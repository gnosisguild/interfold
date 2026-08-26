// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::server::{
    app_data::AppData,
    models::{
        canonical_e3_id, e3_id_to_u256, VoteRequest, VoteResponse, VoteResponseStatus,
        VoteStatusRequest, VoteStatusResponse,
    },
    rate_limit::RateLimiter,
    repo::parse_slot_address,
    CONFIG,
};
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use alloy::primitives::Bytes;
use evm_helpers::{CRISPContract, SimulateError};
use eyre::Error;
use log::{error, info, warn};

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config.service(
        web::scope("/voting")
            .route("/broadcast", web::post().to(broadcast_encrypted_vote))
            .route("/status", web::post().to(get_vote_status)),
    );
}

/// Get the slot activity for an address in a specific round.
///
/// Reports whether the slot holds any published entry, not whether its owner voted: a mask is
/// indistinguishable from a vote by design, and the server does not track who submitted what.
/// A client that wants "did I vote" must remember its own submissions.
///
/// # Arguments
///
/// * `VoteStatusRequest` - The request containing round_id and address
///
/// # Returns
///
/// * A JSON response with the slot activity
async fn get_vote_status(
    data: web::Json<VoteStatusRequest>,
    store: web::Data<AppData>,
) -> impl Responder {
    let request = data.into_inner();
    let e3_id = match canonical_e3_id(&request.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().json(e.to_string()),
    };
    info!(
        "[e3_id={}] Checking slot activity for address: {}",
        e3_id, request.address
    );

    // Validated before any storage access: a malformed address is the client's error, not a
    // database failure.
    let slot = match parse_slot_address(&request.address) {
        Ok(slot) => slot,
        Err(e) => return HttpResponse::BadRequest().json(e.to_string()),
    };

    let slot_active = match store.e3(&e3_id).slot_has_activity(slot).await {
        Ok(active) => active,
        Err(e) => {
            error!(
                "[e3_id={}] Database error checking slot activity: {:?}",
                e3_id, e
            );
            return HttpResponse::InternalServerError().json("Internal server error");
        }
    };

    let round_status = match store.e3(&e3_id).get_e3_state_lite().await {
        Ok(state) => Some(state.status),
        Err(_) => None,
    };

    HttpResponse::Ok().json(VoteStatusResponse {
        round_id: e3_id,
        address: request.address,
        slot_active,
        round_status,
    })
}

/// Broadcast an encrypted vote to the blockchain
///
/// The relay signs and pays for the transaction, so the input is dry-run first — an invalid
/// proof, a stale parent, or a closed window is refused as a client error instead of costing a
/// reverted transaction — and traffic is rate limited per caller and globally.
///
/// # Arguments
///
/// * `EncryptedVote` - The vote data to be broadcast
///
/// # Returns
///
/// * A JSON response indicating the success or failure of the operation
/// Ethereum mainnet, where the relay does not operate.
const MAINNET_CHAIN_ID: u64 = 1;

async fn broadcast_encrypted_vote(
    request: HttpRequest,
    data: web::Json<VoteRequest>,
    limiter: web::Data<RateLimiter>,
) -> impl Responder {
    // No relaying on mainnet: the relay key would pay real gas for anyone who posts a proof,
    // which is an open faucet at mainnet prices. Voters submit `publishInput` from their own
    // wallet there — the function is permissionless and the proof carries everything it needs.
    // Refused before the rate limiter so a refused mainnet call never consumes a window slot.
    if CONFIG.chain_id == MAINNET_CHAIN_ID {
        return HttpResponse::Forbidden().json(VoteResponse {
            status: VoteResponseStatus::FailedBroadcast,
            tx_hash: None,
            message: Some(
                "The relay is disabled on mainnet. Submit the vote directly from your wallet."
                    .to_string(),
            ),
        });
    }
    // Same identity rule as the read routes, and it matters more here: this window is what stops
    // one caller spending the relay's gas. A forgeable key is no key at all — see `caller_id`.
    let caller = super::chain::identify(&request, CONFIG.trust_proxy_headers);

    // Caller admission only. The global transaction quota is reserved after parsing and
    // simulation, right before the relay pays — reserving it here would let invalid requests
    // sprayed across addresses drain it and deny honest voters.
    if limiter.check_caller(&caller).is_err() {
        warn!("Rate limit (caller) refused a broadcast from {caller}");

        return HttpResponse::TooManyRequests().json(VoteResponse {
            status: VoteResponseStatus::FailedBroadcast,
            tx_hash: None,
            message: Some("Too many votes from this address, slow down".to_string()),
        });
    }

    let vote = data.into_inner();
    let e3_id = match e3_id_to_u256(&vote.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().json(e.to_string()),
    };
    let e3_key = e3_id.to_string();

    info!("[e3_id={}] Broadcasting encrypted vote", e3_key);

    // encoded_proof is already encoded in JavaScript, just decode from hex
    let hex_str = vote
        .encoded_proof
        .strip_prefix("0x")
        .unwrap_or(&vote.encoded_proof);
    let encoded_proof = match hex::decode(hex_str) {
        Ok(decoded) => Bytes::from(decoded),
        Err(e) => {
            error!("[e3_id={}] Failed to decode encoded_proof: {:?}", e3_key, e);

            return HttpResponse::BadRequest().json(VoteResponse {
                status: VoteResponseStatus::FailedBroadcast,
                tx_hash: None,
                message: Some("Invalid hex encoded proof".to_string()),
            });
        }
    };

    // Broadcast vote to blockchain
    let contract = match CRISPContract::new(
        &CONFIG.http_rpc_url,
        &CONFIG.private_key,
        &CONFIG.e3_program_address,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            error!("[e3_id={}] Contract creation error: {:?}", e3_key, e);
            return HttpResponse::InternalServerError().json("Internal server error");
        }
    };

    // The dry run: an input the contract would revert must not reach `send`, where the relay
    // pays for the revert. It costs one `eth_call` on inputs that would succeed anyway. Only a
    // revert blames the input — a provider failure judged nothing, so it answers retryable 503
    // rather than telling a voter their valid ballot was refused.
    match contract
        .simulate_publish_input(e3_id, encoded_proof.clone())
        .await
    {
        Ok(()) => {}
        Err(SimulateError::Reverted(reason)) => {
            warn!("[e3_id={}] Input refused by simulation: {}", e3_key, reason);

            return HttpResponse::BadRequest().json(VoteResponse {
                status: VoteResponseStatus::FailedBroadcast,
                tx_hash: None,
                message: Some("Transaction was reverted by the contract".to_string()),
            });
        }
        Err(SimulateError::Provider(reason)) => {
            error!(
                "[e3_id={}] Simulation unavailable (provider failure): {}",
                e3_key, reason
            );

            return HttpResponse::ServiceUnavailable().json(VoteResponse {
                status: VoteResponseStatus::FailedBroadcast,
                tx_hash: None,
                message: Some(
                    "The relay could not reach the blockchain, please try again".to_string(),
                ),
            });
        }
    }

    // The relay is about to pay; this is the point the global quota protects.
    if limiter.try_reserve_global().is_err() {
        warn!("Rate limit (global) refused a broadcast from {caller}");

        return HttpResponse::TooManyRequests().json(VoteResponse {
            status: VoteResponseStatus::FailedBroadcast,
            tx_hash: None,
            message: Some("The relay is busy, please try again shortly".to_string()),
        });
    }

    match contract.publish_input(e3_id, encoded_proof).await {
        Ok(hash) => {
            info!("[e3_id={}] Vote broadcasted successfully", e3_key);
            HttpResponse::Ok().json(VoteResponse {
                status: VoteResponseStatus::Success,
                tx_hash: Some(hash.transaction_hash.to_string()),
                message: Some("Vote Successful".to_string()),
            })
        }
        Err(e) => handle_vote_error(e).await,
    }
}

/// Extract an error message from an error
fn extract_error_message(e: &Error) -> String {
    let error_str = e.to_string();

    if error_str.contains("Internal error") || error_str.contains("-32603") {
        return "Transaction rejected by the blockchain".to_string();
    }
    if error_str.contains("insufficient funds") {
        return "Insufficient funds to process transaction".to_string();
    }
    if error_str.contains("nonce") {
        return "Transaction conflict, please try again".to_string();
    }
    if error_str.contains("gas") {
        return "Transaction failed due to gas issues".to_string();
    }
    if error_str.contains("reverted") {
        return "Transaction was reverted by the contract".to_string();
    }
    if error_str.contains("timeout") || error_str.contains("Timeout") {
        return "Transaction timed out, please try again".to_string();
    }

    "Transaction failed, please try again".to_string()
}

/// Handle the vote error
///
/// # Arguments
///
/// * `e` - The error that occurred
async fn handle_vote_error(e: Error) -> HttpResponse {
    // Log the full error for debugging
    error!("Error while sending vote transaction: {:?}", e);

    let user_message = extract_error_message(&e);

    HttpResponse::InternalServerError().json(VoteResponse {
        status: VoteResponseStatus::FailedBroadcast,
        tx_hash: None,
        message: Some(user_message),
    })
}
