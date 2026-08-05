// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use alloy_primitives::{
    utils::{parse_ether, parse_units},
    Bytes, B256,
};
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolValue;
use anyhow::{Context, Error, Result};
use bincode::serialize;
use boundless_market::{
    client::ClientError,
    contracts::{boundless_market::MarketError, FulfillmentData},
    request_builder::OfferParams,
    storage::storage_provider_from_env,
    Client,
};
use e3_compute_provider::{ComputeInput, ComputeManager, ComputeProvider, FHEInputs};
use e3_support_types::{ComputeDomain, ComputeGuestInput, ComputeJournal};
use e3_user_program::fhe_processor;
use methods::PROGRAM_ELF;
use risc0_ethereum_contracts::groth16;
use risc0_zkvm::{default_prover, ExecutorEnv, ProverOpts, VerifierContext};
use std::error::Error as _;
use std::time::{Duration, Instant};
use url::Url;

pub struct BoundlessProvider {
    domain: ComputeDomain,
}

#[derive(Debug, Clone)]
pub enum BoundlessOutput {
    Success {
        result: ComputeJournal,
        bytes: Vec<u8>,
        seal: Vec<u8>,
    },
    Error {
        error: String,
    },
}

#[derive(Debug)]
pub enum ComputeError {
    BoundlessFailed(String),
    Other(String),
}

impl ComputeProvider for BoundlessProvider {
    type Output = BoundlessOutput;

    fn prove(&self, input: &ComputeInput) -> Self::Output {
        let is_dev_mode =
            std::env::var("RISC0_DEV_MODE").unwrap_or_else(|_| "0".to_string()) == "1";

        if is_dev_mode {
            println!("Dev mode: Using fake proof");
            fake_prove(input, &self.domain)
        } else {
            println!("Using Boundless for proving");
            tokio::runtime::Handle::current().block_on(boundless_prove(input, &self.domain))
        }
    }
}

fn encode_input(input: &[u8]) -> Result<Vec<u8>, Error> {
    Ok(bytemuck::pod_collect_to_vec(&risc0_zkvm::serde::to_vec(
        input,
    )?))
}

fn encode_journal(result: &ComputeJournal) -> Result<Vec<u8>, Error> {
    Ok(bytemuck::pod_collect_to_vec(&risc0_zkvm::serde::to_vec(
        result,
    )?))
}

/// Dev mode: return fake proof without executing
fn fake_prove(input: &ComputeInput, domain: &ComputeDomain) -> BoundlessOutput {
    println!("Generating fake proof for dev mode");

    // Execute the program with the input
    let result = match ComputeJournal::new(domain.clone(), input.process(fhe_processor)) {
        Ok(result) => result,
        Err(error) => return to_output_error(error),
    };

    let journal_bytes = match encode_journal(&result) {
        Ok(bytes) => bytes,
        Err(error) => return to_output_error(error),
    };

    BoundlessOutput::Success {
        result,
        bytes: journal_bytes,
        seal: vec![], // No seal in dev mode
    }
}

fn to_output_error<E: std::fmt::Display>(e: E) -> BoundlessOutput {
    BoundlessOutput::Error {
        error: e.to_string(),
    }
}

/// Read optional environment variable as f64, returning None if unset or invalid.
fn env_opt_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

/// Read optional environment variable as u64 (seconds), returning None if unset or invalid.
fn env_opt_secs(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

/// Build the OfferParams from environment variables, using sensible defaults.
fn build_offer() -> Result<OfferParams> {
    let min_price = if let Some(v) = env_opt_f64("BOUNDLESS_MIN_PRICE_ETH") {
        if v.is_sign_negative() || v.is_nan() {
            anyhow::bail!(
                "BOUNDLESS_MIN_PRICE_ETH must be a non-negative number, got: {}",
                v
            );
        }
        parse_ether(&format!("{}", v)).context("Invalid BOUNDLESS_MIN_PRICE_ETH")?
    } else {
        parse_ether("0.00005").context("Invalid default min_price")?
    };
    let max_price = if let Some(v) = env_opt_f64("BOUNDLESS_MAX_PRICE_ETH") {
        if v.is_sign_negative() || v.is_nan() {
            anyhow::bail!(
                "BOUNDLESS_MAX_PRICE_ETH must be a non-negative number, got: {}",
                v
            );
        }
        parse_ether(&format!("{}", v)).context("Invalid BOUNDLESS_MAX_PRICE_ETH")?
    } else {
        parse_ether("0.002").context("Invalid default max_price")?
    };
    let timeout = env_opt_secs("BOUNDLESS_TIMEOUT_SECS")
        .map(|v| v as u32)
        .unwrap_or(10 * 60);
    let lock_timeout = env_opt_secs("BOUNDLESS_LOCK_TIMEOUT_SECS")
        .map(|v| v as u32)
        .unwrap_or(5 * 60);
    let ramp_up = env_opt_secs("BOUNDLESS_RAMP_UP_SECS")
        .map(|v| v as u32)
        .unwrap_or(1 * 60);
    let zkc = env_opt_f64("BOUNDLESS_LOCK_COLLATERAL_ZKC").unwrap_or(2.0);
    if zkc.is_sign_negative() || zkc.is_nan() {
        anyhow::bail!(
            "BOUNDLESS_LOCK_COLLATERAL_ZKC must be a non-negative number, got: {}",
            zkc
        );
    }
    let collateral: alloy_primitives::U256 = parse_units(&format!("{}", zkc), 18)
        .context("Invalid BOUNDLESS_LOCK_COLLATERAL_ZKC")?
        .into();

    Ok(OfferParams::builder()
        .min_price(min_price)
        .max_price(max_price)
        .timeout(timeout)
        .lock_timeout(lock_timeout)
        .ramp_up_period(ramp_up)
        .lock_collateral(collateral)
        .into())
}

async fn boundless_prove(input: &ComputeInput, domain: &ComputeDomain) -> BoundlessOutput {
    match boundless_prove_inner(input, domain).await {
        Ok(output) => output,
        Err(e) => {
            // Print the full error chain so the root cause is visible in logs.
            eprintln!("✗ Boundless proof request FAILED:");
            eprintln!("  Error: {:#}", e);
            let mut source = e.source();
            while let Some(s) = source {
                eprintln!("  Caused by: {}", s);
                source = s.source();
            }
            to_output_error(e)
        }
    }
}

async fn boundless_prove_inner(
    input: &ComputeInput,
    domain: &ComputeDomain,
) -> Result<BoundlessOutput> {
    println!("Submitting proof request to Boundless...");

    let rpc_url = std::env::var("RPC_URL")
        .context("RPC_URL not set")?
        .parse()
        .context("Invalid RPC_URL")?;

    let private_key: PrivateKeySigner = std::env::var("PRIVATE_KEY")
        .context("PRIVATE_KEY not set")?
        .parse()
        .context("Invalid PRIVATE_KEY")?;

    let storage_provider = match storage_provider_from_env() {
        Ok(provider) => Some(provider),
        Err(e) => {
            eprintln!("Warning: Failed to get storage provider: {}", e);
            None
        }
    };

    // Diagnostic: log what we're connecting to (key and API path never logged).
    println!(
        "Boundless client: caller={}, storage_provider={}",
        private_key.address(),
        storage_provider.is_some(),
    );

    let client = Client::builder()
        .with_rpc_url(rpc_url)
        .with_private_key(private_key)
        .with_storage_provider(storage_provider)
        .build()
        .await
        .context("Failed to build Boundless client")?;

    let guest_input = ComputeGuestInput {
        domain: domain.clone(),
        input: input.clone(),
    };
    let serialized_input = serialize(&guest_input).context("Failed to serialize guest input")?;
    let input_bytes = encode_input(&serialized_input).context("Failed to encode input")?;

    let program_url = std::env::var("PROGRAM_URL").ok();
    let stdin_size = input_bytes.len();

    let request = if let Some(ref url) = program_url {
        println!("Using pre-uploaded program: {}", url);
        let parsed_url = url.parse::<Url>().context("Failed to parse program URL")?;

        client
            .new_request()
            .with_program_url(parsed_url)
            .context("Failed to create new request")?
            .with_stdin(input_bytes)
            .with_offer(build_offer()?)
    } else {
        println!(
            "Warning: Uploading {}MB program at runtime",
            PROGRAM_ELF.len() / 1_000_000
        );
        client
            .new_request()
            .with_program(PROGRAM_ELF)
            .with_stdin(input_bytes)
            .with_offer(build_offer()?)
    };

    let onchain =
        std::env::var("BOUNDLESS_ONCHAIN").unwrap_or_else(|_| "true".to_string()) == "true";

    println!(
        "Boundless submission: onchain={}, program_url={:?}, stdin_size={}",
        onchain, program_url, stdin_size,
    );

    let (request_id, expires_at) = if onchain {
        println!("Building request...");
        let proof_request = match client.build_request(request).await {
            Ok(r) => {
                println!("✓ Request built successfully (id: {:x})", r.id);
                r
            }
            Err(e) => {
                eprintln!("✗ Build request FAILED:");
                eprintln!("  Debug: {:?}", e);
                eprintln!("  Display: {:#}", e);
                let mut source = e.source();
                while let Some(s) = source {
                    eprintln!("  Caused by: {}", s);
                    source = s.source();
                }
                return Err(anyhow::anyhow!("Failed to build request: {:#}", e));
            }
        };

        println!("Submitting onchain (request id: {:x})...", proof_request.id);
        match client.submit_request_onchain(&proof_request).await {
            Ok(result) => {
                println!("✓ Onchain submission successful");
                result
            }
            Err(e) => {
                eprintln!("✗ Onchain submission FAILED:");
                eprintln!("  Display: {:#}", e);
                let mut source = e.source();
                while let Some(s) = source {
                    eprintln!("  Caused by: {}", s);
                    source = s.source();
                }
                return Err(anyhow::anyhow!("Failed to submit onchain: {:#}", e));
            }
        }
    } else {
        println!("Submitting offchain...");
        match client.submit_offchain(request).await {
            Ok(result) => {
                println!("✓ Offchain submission successful");
                result
            }
            Err(e) => {
                eprintln!("✗ Offchain submission FAILED:");
                eprintln!("  Error: {:#}", e);
                let mut source = e.source();
                while let Some(s) = source {
                    eprintln!("  Caused by: {}", s);
                    source = s.source();
                }
                return Err(anyhow::anyhow!("Failed to submit offchain: {:#}", e));
            }
        }
    };

    println!("Request ID: {:x}, waiting for fulfillment...", request_id);

    let fulfillment = match client
        .wait_for_request_fulfillment(request_id, Duration::from_secs(5), expires_at)
        .await
    {
        Ok(fulfillment) => fulfillment,
        Err(ClientError::MarketError(MarketError::RequestHasExpired(_))) => {
            return Ok(BoundlessOutput::Error {
                error: format!(
                    "Boundless request expired: no prover picked up the request. Request ID: {:x}",
                    request_id
                ),
            });
        }
        Err(e) => return Err(e).context("Failed to wait for fulfillment")?,
    };

    println!("Proof received from Boundless!");
    let data = fulfillment.data();
    let (_, journal) = match data {
        Ok(FulfillmentData::ImageIdAndJournal(image_id, journal)) => (image_id, journal),
        _ => {
            return Ok(BoundlessOutput::Error {
                error: "Invalid fulfillment data".to_string(),
            });
        }
    };

    let decoded_journal: ComputeJournal = risc0_zkvm::serde::from_slice(&journal)
        .map_err(|e| anyhow::anyhow!("Failed to decode journal: {}", e))?;

    Ok(BoundlessOutput::Success {
        result: decoded_journal,
        bytes: journal.to_vec(),
        seal: fulfillment.seal.to_vec(),
    })
}

pub struct Risc0Provider {
    domain: ComputeDomain,
}

#[derive(Debug, Clone)]
pub struct Risc0Output {
    pub result: ComputeJournal,
    pub bytes: Vec<u8>,
    pub seal: Vec<u8>,
}

impl ComputeProvider for Risc0Provider {
    type Output = Risc0Output;

    fn prove(&self, input: &ComputeInput) -> Self::Output {
        let guest_input = ComputeGuestInput {
            domain: self.domain.clone(),
            input: input.clone(),
        };
        let encoded_input = encode_input(&serialize(&guest_input).unwrap()).unwrap();
        let env = ExecutorEnv::builder()
            .write_slice(&encoded_input)
            .build()
            .unwrap();

        let receipt = default_prover()
            .prove_with_ctx(
                env,
                &VerifierContext::default(),
                PROGRAM_ELF,
                &ProverOpts::groth16(),
            )
            .unwrap()
            .receipt;

        let decoded_journal: ComputeJournal = receipt.journal.decode().unwrap();

        // Check if RISC0_DEV_MODE is set to "1" (dev mode)
        // If dev mode: return empty seal (fake proof)
        // Otherwise: return real groth16 proof
        let is_dev_mode = std::env::var("RISC0_DEV_MODE").unwrap_or_default() == "1";

        let seal = if is_dev_mode {
            println!("RISC0_DEV_MODE=1: Using fake proof (empty seal)");
            vec![]
        } else {
            println!("RISC0_DEV_MODE=0 or unset: Generating real Groth16 proof");
            groth16::encode(receipt.inner.groth16().unwrap().seal.clone()).unwrap()
        };

        Risc0Output {
            result: decoded_journal,
            bytes: receipt.journal.bytes.clone(),
            seal,
        }
    }
}

pub fn run_compute(
    params: FHEInputs,
    domain: ComputeDomain,
) -> std::result::Result<(BoundlessOutput, Vec<u8>), ComputeError> {
    let boundless_provider = BoundlessProvider { domain };

    let mut provider = ComputeManager::new(
        boundless_provider,
        params.clone(),
        fhe_processor,
        false,
        None,
    );

    // Start timer
    let start_time = Instant::now();

    let output = provider.start();

    // Capture end time and calculate the duration
    let elapsed_time = start_time.elapsed();

    // Convert the elapsed time to minutes and seconds
    let minutes = elapsed_time.as_secs() / 60;
    let seconds = elapsed_time.as_secs() % 60;

    println!(
        "Prove function execution time: {} minutes and {} seconds",
        minutes, seconds
    );

    // Check if the output indicates failure
    match output.0 {
        BoundlessOutput::Success { .. } => Ok(output),
        BoundlessOutput::Error { error } => Err(ComputeError::BoundlessFailed(error)),
    }
}

pub fn run_risc0_compute(
    params: FHEInputs,
    domain: ComputeDomain,
) -> std::result::Result<(Risc0Output, Vec<u8>), ComputeError> {
    let risc0_provider = Risc0Provider { domain };

    let mut provider =
        ComputeManager::new(risc0_provider, params.clone(), fhe_processor, false, None);

    let output = provider.start();

    Ok(output)
}

pub fn encode_compute_proof(
    seal: &[u8],
    result: &ComputeJournal,
) -> std::result::Result<Vec<u8>, ComputeError> {
    if result.params_hash.len() != 32 || result.merkle_root.len() != 32 {
        return Err(ComputeError::Other(
            "Compute journal context must contain two 32-byte values".to_string(),
        ));
    }
    Ok((
        Bytes::copy_from_slice(seal),
        B256::from_slice(&result.params_hash),
        B256::from_slice(&result.merkle_root),
    )
        .abi_encode_params())
}

#[cfg(test)]
mod tests {
    use super::*;
    use risc0_zkvm::sha::{Impl, Sha256};

    fn risc0_vec32(value: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(132);
        encoded.extend_from_slice(&32_u32.to_le_bytes());
        for byte in value {
            encoded.extend_from_slice(&u32::from(*byte).to_le_bytes());
        }
        encoded
    }

    #[test]
    fn compute_result_journal_matches_crisp_layout() {
        let domain = ComputeDomain::new(
            31_337,
            "0x1111111111111111111111111111111111111111",
            7,
            &[0x22; 32],
            &[0x33; 32],
        )
        .unwrap();
        let result = ComputeJournal::new(
            domain,
            e3_compute_provider::ComputeResult {
                ciphertext_hash: (0_u8..32).collect(),
                ciphertext_commitment: (32_u8..64).collect(),
                params_hash: hex::decode(
                    "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470",
                )
                .unwrap(),
                merkle_root: hex::decode(
                    "2134e76ac5d21aab186c2be1dd8f84ee880a1e46eaf712f9d371b6df22191f3e",
                )
                .unwrap(),
            },
        )
        .unwrap();

        let journal = encode_journal(&result).expect("journal encoding failed");
        let expected = [
            result.chain_id.as_slice(),
            result.verifying_contract.as_slice(),
            result.e3_id.as_slice(),
            result.encryption_scheme_id.as_slice(),
            result.committee_public_key_hash.as_slice(),
            result.ciphertext_hash.as_slice(),
            result.ciphertext_commitment.as_slice(),
            result.params_hash.as_slice(),
            result.merkle_root.as_slice(),
        ]
        .into_iter()
        .flat_map(risc0_vec32)
        .collect::<Vec<_>>();

        assert_eq!(journal.len(), 1188);
        assert_eq!(journal, expected);
        assert_eq!(
            hex::encode(Impl::hash_bytes(&journal).as_bytes()),
            "4403934eb9404372d77f23454aeb4bb7f21bbe856c5c51fc3243f5e05cc2c702"
        );
    }

    #[test]
    fn compute_proof_uses_solidity_parameter_encoding() {
        let result = ComputeJournal {
            chain_id: vec![0; 32],
            verifying_contract: vec![0; 32],
            e3_id: vec![0; 32],
            encryption_scheme_id: vec![0; 32],
            committee_public_key_hash: vec![0; 32],
            ciphertext_hash: vec![0; 32],
            ciphertext_commitment: vec![0; 32],
            params_hash: vec![0x22; 32],
            merkle_root: vec![0x33; 32],
        };

        let encoded = encode_compute_proof(&[0x11; 4], &result).unwrap();
        let (seal, params_hash, input_root) =
            <(Bytes, B256, B256)>::abi_decode_params(&encoded).unwrap();

        assert_eq!(seal.as_ref(), &[0x11; 4]);
        assert_eq!(params_hash, B256::repeat_byte(0x22));
        assert_eq!(input_root, B256::repeat_byte(0x33));
    }
}
