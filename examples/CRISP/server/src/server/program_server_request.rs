// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::server::CONFIG;

use anyhow::Result;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Serialize)]
pub struct ComputeRequest {
    pub e3_id: Option<u64>,
    pub chain_id: u64,
    pub interfold_address: String,
    #[serde(serialize_with = "serialize_as_hex")]
    pub encryption_scheme_id: Vec<u8>,
    #[serde(serialize_with = "serialize_as_hex")]
    pub committee_public_key_hash: Vec<u8>,
    #[serde(serialize_with = "serialize_as_hex")]
    pub params: Vec<u8>,
    #[serde(serialize_with = "serialize_hex_tuple")]
    pub ciphertext_inputs: Vec<(Vec<u8>, u64)>,
    pub callback_url: Option<String>,
}

fn serialize_as_hex<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let hex_string = format!("0x{}", hex::encode(bytes));
    serializer.serialize_str(&hex_string)
}

fn serialize_hex_tuple<S>(tuples: &[(Vec<u8>, u64)], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let hex_tuples: Vec<(String, u64)> = tuples
        .iter()
        .map(|(bytes, num)| (format!("0x{}", hex::encode(bytes)), *num))
        .collect();
    hex_tuples.serialize(serializer)
}

#[derive(Deserialize, Serialize)]
pub struct ProcessingResponse {
    pub status: String,
    pub e3_id: u64,
}

fn build_compute_request(
    client: &reqwest::Client,
    program_server_url: &str,
    request: &ComputeRequest,
) -> reqwest::RequestBuilder {
    client
        .post(format!("{program_server_url}/run_compute"))
        .json(request)
}

pub async fn run_compute(
    e3_id: u64,
    chain_id: u64,
    interfold_address: String,
    encryption_scheme_id: Vec<u8>,
    committee_public_key_hash: Vec<u8>,
    params: Vec<u8>,
    ciphertext_inputs: Vec<(Vec<u8>, u64)>,
    webhook_url: String,
) -> Result<(u64, String)> {
    let request = ComputeRequest {
        e3_id: Some(e3_id),
        chain_id,
        interfold_address,
        encryption_scheme_id,
        committee_public_key_hash,
        callback_url: Some(webhook_url),
        params,
        ciphertext_inputs,
    };

    println!("Sending request");

    let response: ProcessingResponse = build_compute_request(
        &reqwest::Client::new(),
        &CONFIG.program_server_url,
        &request,
    )
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;

    Ok((response.e3_id, response.status))
}

#[cfg(test)]
mod tests {
    use reqwest::header::AUTHORIZATION;

    use super::{build_compute_request, ComputeRequest};

    #[test]
    fn compute_request_does_not_require_authorization() {
        let request = ComputeRequest {
            e3_id: Some(7),
            chain_id: 31_337,
            interfold_address: "0x1111111111111111111111111111111111111111".to_string(),
            encryption_scheme_id: vec![0x22; 32],
            committee_public_key_hash: vec![0x33; 32],
            params: vec![1, 2, 3],
            ciphertext_inputs: vec![],
            callback_url: Some("http://127.0.0.1:4000/state/add-result".to_string()),
        };

        let request =
            build_compute_request(&reqwest::Client::new(), "http://127.0.0.1:13151", &request)
                .build()
                .expect("request should build");

        assert_eq!(request.url().as_str(), "http://127.0.0.1:13151/run_compute");
        assert!(!request.headers().contains_key(AUTHORIZATION));
    }
}
