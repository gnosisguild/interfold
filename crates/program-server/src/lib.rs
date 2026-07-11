// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

mod types;

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer, Result as ActixResult};
use anyhow::{Context, Result};
use e3_compute_provider::FHEInputs;
use serde::Serialize;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use types::{ComputeRequest, WebhookPayload};

#[derive(Serialize, Debug)]
struct ProcessingResponse {
    status: String,
    e3_id: u64,
}

type RunnerResult = Result<(Vec<u8>, Vec<u8>)>;
type Runner = dyn Fn(FHEInputs) -> Pin<Box<dyn Future<Output = RunnerResult> + Send>> + Send + Sync;

#[derive(Clone)]
pub struct E3ProgramServerBuilder {
    runner: Arc<Runner>,
    port: Option<u16>,
    host: Option<String>,
    localhost_rewrite: Option<String>,
    max_concurrent_jobs: usize,
}

impl E3ProgramServerBuilder {
    /// Create a new builder with a computation callback
    pub fn new<F, Fut>(callback: F) -> Self
    where
        F: Fn(FHEInputs) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RunnerResult> + Send + 'static,
    {
        Self {
            runner: Arc::new(move |inputs| Box::pin(callback(inputs))),
            port: None,
            host: None,
            localhost_rewrite: None,
            max_concurrent_jobs: 1,
        }
    }

    /// Set the port number (default: 13151)
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the host address (default: "0.0.0.0")
    pub fn with_host<S: Into<String>>(mut self, host: S) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Server will rewrite localhost callbacks to whatever is provided as an argument eg. "host.local". This is usefull when running in a Docker container which does not have direct access to the host
    pub fn with_localhost_rewrite(mut self, rewrite: &str) -> Self {
        self.localhost_rewrite = Some(rewrite.to_string());
        self
    }

    /// Bound the number of computations that may execute concurrently.
    pub fn with_max_concurrent_jobs(mut self, max_concurrent_jobs: usize) -> Self {
        self.max_concurrent_jobs = max_concurrent_jobs;
        self
    }

    /// Build the E3ProgramServer
    pub fn build(self) -> Result<E3ProgramServer> {
        anyhow::ensure!(
            self.max_concurrent_jobs > 0,
            "max concurrent jobs must be greater than zero"
        );
        let webhook_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("failed to build webhook client")?;
        Ok(E3ProgramServer {
            runner: self.runner,
            port: self.port.unwrap_or(13151),
            host: self.host.unwrap_or_else(|| "0.0.0.0".to_string()),
            localhost_rewrite: self.localhost_rewrite,
            webhook_client,
            jobs: Arc::new(Semaphore::new(self.max_concurrent_jobs)),
        })
    }
}

#[derive(Clone)]
pub struct E3ProgramServer {
    runner: Arc<Runner>,
    port: u16,
    host: String,
    localhost_rewrite: Option<String>,
    webhook_client: reqwest::Client,
    jobs: Arc<Semaphore>,
}

impl E3ProgramServer {
    /// Create a new builder for E3ProgramServer with a computation callback
    pub fn builder<F, Fut>(callback: F) -> E3ProgramServerBuilder
    where
        F: Fn(FHEInputs) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RunnerResult> + Send + 'static,
    {
        E3ProgramServerBuilder::new(callback)
    }

    /// Get the configured port
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the configured host
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the bind address as a string
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Run the HTTP server
    pub async fn run(&self) -> Result<()> {
        let bind_addr = self.bind_address();
        let config = AppConfig {
            runner: Arc::clone(&self.runner),
            localhost_rewrite: self.localhost_rewrite.clone(),
            webhook_client: self.webhook_client.clone(),
            jobs: Arc::clone(&self.jobs),
        };
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(config.clone()))
                .app_data(web::JsonConfig::default().limit(10 * 1024 * 1024)) // 10MB for prod params
                .wrap(Logger::default())
                .route("/run_compute", web::post().to(handle_compute))
                .route("/health", web::get().to(handle_health_check))
                .route("/health", web::head().to(handle_health_check))
        })
        .bind(&bind_addr)?;

        println!("🚀 E3 Program Server listening on http://{}", bind_addr);
        server.run().await.map_err(Into::into)
    }
}

#[derive(Clone)]
pub struct AppConfig {
    pub runner: Arc<Runner>,
    pub localhost_rewrite: Option<String>,
    webhook_client: reqwest::Client,
    jobs: Arc<Semaphore>,
}

fn parse_http_url(value: &str, label: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(value).with_context(|| format!("invalid {label}"))?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https"),
        "{label} must use http or https"
    );
    anyhow::ensure!(url.host_str().is_some(), "{label} must contain a host");
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none(),
        "{label} must not contain credentials"
    );
    Ok(url)
}

fn validated_callback_url(
    callback_url: &str,
    localhost_rewrite: Option<&str>,
) -> Result<reqwest::Url> {
    let mut callback = parse_http_url(callback_url, "callback URL")?;
    if matches!(callback.host_str(), Some("localhost" | "127.0.0.1")) {
        if let Some(rewrite) = localhost_rewrite {
            callback
                .set_host(Some(rewrite))
                .map_err(|_| anyhow::anyhow!("invalid localhost rewrite host"))?;
        }
    }
    anyhow::ensure!(
        callback.fragment().is_none(),
        "callback URL must not contain a fragment"
    );
    Ok(callback)
}

async fn call_webhook(
    client: &reqwest::Client,
    callback_url: &reqwest::Url,
    payload: WebhookPayload,
) -> Result<()> {
    let e3_id = match &payload {
        WebhookPayload::Completed { e3_id, .. } => *e3_id,
        WebhookPayload::Failed { e3_id, .. } => *e3_id,
    };

    match &payload {
        WebhookPayload::Completed {
            ciphertext, proof, ..
        } => {
            println!(
                "call_webhook() - status: Completed, ciphertext len: {}, proof len: {}",
                ciphertext.len(),
                proof.len()
            );
        }
        WebhookPayload::Failed { error, .. } => {
            println!("call_webhook() - status: Failed, error: {}", error);
        }
    }

    let response = client
        .post(callback_url.clone())
        .json(&payload)
        .send()
        .await?;

    println!("Webhook response status: {}", response.status());
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Webhook failed with status {}",
            response.status()
        ));
    }

    response.error_for_status()?;
    println!("✓ Webhook called successfully for E3 {}", e3_id);
    Ok(())
}

async fn handle_webhook_delivery(
    client: &reqwest::Client,
    callback_url: &reqwest::Url,
    payload: WebhookPayload,
) -> Result<()> {
    println!("handle_webhook_delivery()");
    call_webhook(client, callback_url, payload).await?;
    println!("✓ Webhook sent successfully");
    Ok(())
}

async fn process_computation_background(
    runner: Arc<Runner>,
    e3_id: u64,
    webhook_client: reqwest::Client,
    callback_url: reqwest::Url,
    fhe_inputs: FHEInputs,
) -> Result<()> {
    match runner(fhe_inputs).await {
        Ok((proof, ciphertext)) => {
            println!("computation finished!");
            println!("handling webhook delivery...");
            let payload = WebhookPayload::Completed {
                e3_id,
                ciphertext,
                proof,
            };
            handle_webhook_delivery(&webhook_client, &callback_url, payload).await?;
            println!("✓ Computation completed for E3 {}", e3_id);
            Ok(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            eprintln!("Computation failed for E3 {}: {}", e3_id, error_msg);

            let payload = WebhookPayload::Failed {
                e3_id,
                error: format!("Compute failed: {}", error_msg),
            };
            handle_webhook_delivery(&webhook_client, &callback_url, payload).await?;

            Err(e)
        }
    }
}

async fn handle_compute(
    config: web::Data<AppConfig>,
    req: web::Json<ComputeRequest>,
) -> ActixResult<HttpResponse> {
    println!("Processing computation...");
    let e3_id = req
        .e3_id
        .ok_or_else(|| actix_web::error::ErrorBadRequest("e3_id is required"))?;

    let callback_url = req
        .callback_url
        .clone()
        .ok_or_else(|| actix_web::error::ErrorBadRequest("callback_url is required"))?;

    let fhe_inputs = FHEInputs {
        params: req.params.clone(),
        ciphertexts: req.ciphertext_inputs.clone(),
    };

    let callback_url = validated_callback_url(&callback_url, config.localhost_rewrite.as_deref())
        .map_err(actix_web::error::ErrorBadRequest)?;
    let permit = Arc::clone(&config.jobs)
        .try_acquire_owned()
        .map_err(|_| actix_web::error::ErrorTooManyRequests("compute capacity exhausted"))?;
    let runner = config.runner.clone();
    let webhook_client = config.webhook_client.clone();
    tokio::spawn(async move {
        let _permit = permit;
        if let Err(e) =
            process_computation_background(runner, e3_id, webhook_client, callback_url, fhe_inputs)
                .await
        {
            eprintln!("✗ Background computation failed for E3 {}: {:?}", e3_id, e);
        }
    });

    Ok(HttpResponse::Ok().json(ProcessingResponse {
        status: "processing".to_string(),
        e3_id,
    }))
}

async fn handle_health_check() -> ActixResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(ProcessingResponse {
        status: "healthy".to_string(),
        e3_id: 0,
    }))
}

#[cfg(test)]
mod server_tests {
    use super::*;

    #[test]
    fn callback_validation_accepts_http_origins_and_rejects_unsafe_urls() {
        assert!(validated_callback_url("https://callback.example:8443/results/1", None).is_ok());
        assert!(validated_callback_url("https://metadata.internal:8443/latest", None).is_ok());
        assert!(validated_callback_url("file:///etc/passwd", None).is_err());
        assert!(validated_callback_url("https://user:pass@example.com/result", None).is_err());
        assert!(validated_callback_url("https://example.com/result#fragment", None).is_err());
    }

    #[test]
    fn builder_rejects_zero_capacity_and_configures_the_job_limit() {
        let zero = E3ProgramServer::builder(|_| async { Ok((vec![], vec![])) })
            .with_max_concurrent_jobs(0)
            .build();
        assert!(zero.is_err());

        let server = E3ProgramServer::builder(|_| async { Ok((vec![], vec![])) })
            .with_max_concurrent_jobs(2)
            .build()
            .unwrap();
        assert_eq!(server.jobs.available_permits(), 2);
    }

    #[test]
    fn localhost_rewrite_changes_only_an_exact_local_host() {
        let rewritten =
            validated_callback_url("http://127.0.0.1:8080/result", Some("host.local")).unwrap();
        assert_eq!(rewritten.as_str(), "http://host.local:8080/result");
        let unchanged =
            validated_callback_url("http://localhost.attacker:8080/result", Some("host.local"))
                .unwrap();
        assert_eq!(unchanged.as_str(), "http://localhost.attacker:8080/result");
    }
}
