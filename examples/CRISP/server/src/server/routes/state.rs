// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::str::FromStr;

use crate::server::{
    app_data::AppData,
    models::{
        canonical_e3_id, e3_id_to_u256, GetRoundRequest, PreviousCiphertextRequest,
        PreviousCiphertextResponse, RoundRequestWithRequester, WebhookPayload,
    },
    CONFIG,
};
use actix_web::{web, HttpResponse, Responder};
use alloy::primitives::{Address, Bytes, B256};
use e3_sdk::evm_helpers::contracts::{
    InterfoldContract, InterfoldContractFactory, InterfoldWrite, ReadWrite,
};
use log::{error, info};

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config.service(
        web::scope("/state")
            .route("/result", web::post().to(get_round_result))
            .route("/all", web::post().to(get_all_round_results))
            .route("/lite", web::post().to(get_round_state_lite))
            // Do we need protection on this endpoint? technically they would need to send a valid proof for it to
            // be included on chain
            .route("/add-result", web::post().to(handle_program_server_result))
            // Get the token holders hashes for a given round
            .route("/token-holders", web::post().to(get_token_holders_hashes))
            .route(
                "/eligible-addresses",
                web::post().to(handle_get_eligible_addresses),
            )
            .route(
                "/previous-ciphertext",
                web::post().to(handle_get_previous_ciphertext),
            ),
    );
}

/// Endpoint to get the ciphertext a slot currently holds. Used for every ballot, not only masks.
///
/// Answers with the end of the slot's chain of usable entries, and the tree index of that entry.
/// Not simply the newest entry published: an entry whose bytes do not reproduce its commitment is
/// never selected by the Secure Process and is never a valid parent, so building on it would have
/// the client's input dropped from the tally.
///
/// # Arguments
/// * `data` - The round id and the slot address
///
/// # Returns
/// * A JSON response with the ciphertext and its index, or 404 when the slot holds nothing usable.
async fn handle_get_previous_ciphertext(
    data: web::Json<PreviousCiphertextRequest>,
    store: web::Data<AppData>,
) -> impl Responder {
    let incoming = data.into_inner();

    let e3_id = match e3_id_to_u256(&incoming.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };
    let e3_key = e3_id.to_string();

    let address = match Address::from_str(incoming.address.as_str()) {
        Ok(addr) => addr,
        Err(e) => {
            error!("Invalid address format: {:?}", e);
            return HttpResponse::BadRequest().body("Invalid address format");
        }
    };

    // No BFV work and no parameters here. Whether an entry's bytes reproduce its commitment is
    // decided once, when the indexer stores it, so resolving the chain is a walk over flags.
    match store.e3(e3_key).get_slot_head(address.into()).await {
        Ok(Some((ciphertext, index))) => {
            HttpResponse::Ok().json(PreviousCiphertextResponse { ciphertext, index })
        }
        Ok(None) => HttpResponse::NotFound().body("Ciphertext not found"),
        Err(e) => {
            error!("Error getting previous ciphertext: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to get previous ciphertext")
        }
    }
}

/// Webhook callback from program server
///
/// # Arguments
/// * `data` - The request data containing the result from the program server
///
/// # Returns
/// * A JSON response indicating the success of the operation
async fn handle_program_server_result(data: web::Json<WebhookPayload>) -> impl Responder {
    let incoming = data.into_inner();

    match incoming {
        WebhookPayload::Failed { e3_id, error } => {
            error!("Computation failed for E3 ID: {}. Error: {}", e3_id, error);

            // TODO: Update E3 state to indicate computation failed
            // TODO: Handle ciphernode rewards for partial work
            // TODO: Emit on-chain event if needed

            HttpResponse::Ok().json(format!(
                "Computation failed for E3 ID: {}. Error: {}",
                e3_id, error
            ))
        }
        WebhookPayload::Completed {
            e3_id,
            ciphertext,
            ciphertext_commitment,
            proof,
        } => {
            info!(
                "Received program server result for E3 ID: {}, ciphertext len: {}, proof len: {}",
                e3_id,
                ciphertext.len(),
                proof.len()
            );

            // In dev mode, proof might be empty
            if ciphertext.is_empty() && proof.is_empty() {
                info!(
                    "Both ciphertext and proof are empty for E3 ID: {} - skipping chain publication",
                    e3_id
                );
                return HttpResponse::Ok()
                    .json(format!("Computation completed for E3 ID: {}", e3_id));
            }

            if ciphertext_commitment.len() != 32 {
                return HttpResponse::BadRequest()
                    .body("ciphertext_commitment must be exactly 32 bytes");
            }

            // Create the contract
            let contract: InterfoldContract<ReadWrite> =
                match InterfoldContractFactory::create_write(
                    &CONFIG.http_rpc_url,
                    &CONFIG.interfold_address,
                    &CONFIG.private_key,
                )
                .await
                {
                    Ok(contract) => contract,
                    Err(e) => {
                        error!("Failed to create contract: {:?}", e);
                        return HttpResponse::InternalServerError()
                            .json(format!("Failed to create contract: {}", e));
                    }
                };

            // Try the direct call
            let tx_result = contract
                .publish_ciphertext_output(
                    match e3_id_to_u256(&e3_id) {
                        Ok(e3_id) => e3_id,
                        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
                    },
                    Bytes::from(ciphertext.clone()),
                    B256::from_slice(&ciphertext_commitment),
                    Bytes::from(proof.clone()),
                )
                .await;

            let pending_tx = match tx_result {
                Ok(tx) => tx,
                Err(e) => {
                    error!("Failed to send transaction: {:?}", e);
                    return HttpResponse::InternalServerError()
                        .json(format!("Failed to send transaction: {}", e));
                }
            };

            info!(
                "Ciphertext output published successfully for E3 ID: {} with tx: {}",
                e3_id, pending_tx.transaction_hash
            );

            HttpResponse::Ok().json(format!(
                "Ciphertext output published successfully for E3 ID: {}",
                e3_id
            ))
        }
    }
}

/// Get the result for a given round
///
/// # Arguments
///
/// * `GetRoundRequest` - The request data containing the round ID
///
/// # Returns
///
async fn get_round_result(
    data: web::Json<GetRoundRequest>,
    store: web::Data<AppData>,
) -> impl Responder {
    let incoming = data.into_inner();
    let e3_id = match canonical_e3_id(&incoming.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    match store.e3(e3_id).get_web_result_request().await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => {
            error!("Error getting E3 state: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to get E3 state")
        }
    }
}

/// Get all the results for all rounds
///
/// # Returns
///
/// * A JSON response containing the results for all rounds
async fn get_all_round_results(
    data: web::Json<RoundRequestWithRequester>,
    store: web::Data<AppData>,
) -> impl Responder {
    let incoming = data.into_inner();

    let round_ids = match store.current_round().get_round_ids().await {
        Ok(ids) => ids,
        Err(e) => {
            info!("Error retrieving round index: {:?}", e);
            return HttpResponse::InternalServerError().body("Failed to retrieve round index");
        }
    };

    let mut states = Vec::new();
    let requesters = incoming.requesters;

    for e3_id in round_ids {
        match store.e3(&e3_id).get_web_result_request().await {
            Ok(w) => {
                if !requesters.is_empty() {
                    // if we have any requesters to filter by, do it
                    if requesters.contains(&w.requester) {
                        states.push(w);
                    }
                } else {
                    states.push(w);
                }
            }
            Err(e) => {
                // Expected for a round whose committee key is not published yet — the `_e3:`
                // record only exists after CommitteePublished. See the note in
                // `get_current_round_for_requester`.
                info!(
                    "Round {} is not fully indexed yet (usually: committee key pending) — skipping: {:?}",
                    e3_id, e
                );
                continue;
            }
        }
    }

    HttpResponse::Ok().json(states)
}

/// Get the state for a given round
///
/// # Arguments
///
/// * `GetRoundRequest` - The request data containing the round ID
///
/// # Returns
///
async fn get_round_state_lite(
    data: web::Json<GetRoundRequest>,
    store: web::Data<AppData>,
) -> impl Responder {
    let incoming = data.into_inner();
    let e3_id = match canonical_e3_id(&incoming.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    match store.e3(e3_id).get_e3_state_lite().await {
        Ok(state_lite) => HttpResponse::Ok().json(state_lite),
        Err(_) => HttpResponse::InternalServerError().body("Failed to get E3 state"),
    }
}

/// Get the hashes of token holders for a given round
/// The hash is hash(address, token balance)
/// # Arguments
/// * `GetRoundRequest` - The request data containing the round ID
/// # Returns
/// * A JSON response containing the list of token holder hashes
async fn get_token_holders_hashes(
    data: web::Json<GetRoundRequest>,
    store: web::Data<AppData>,
) -> impl Responder {
    let incoming = data.into_inner();
    let e3_id = match canonical_e3_id(&incoming.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    match store.e3(e3_id).get_token_holder_hashes().await {
        Ok(hashes) => HttpResponse::Ok().json(hashes),
        Err(e) => {
            error!("Error getting token holders hashes: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to get token holders hashes")
        }
    }
}

/// Get the eligible addresses for a given round
/// # Arguments
/// * `GetRoundRequest` - The request data containing the round ID
/// # Returns
/// * A JSON response containing the list of eligible addresses and their balances
async fn handle_get_eligible_addresses(
    data: web::Json<GetRoundRequest>,
    store: web::Data<AppData>,
) -> impl Responder {
    let incoming = data.into_inner();
    let e3_id = match canonical_e3_id(&incoming.round_id) {
        Ok(e3_id) => e3_id,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    match store.e3(e3_id).get_eligible_addresses().await {
        Ok(addresses) => HttpResponse::Ok().json(addresses),
        Err(e) => {
            error!("Error getting eligible addresses: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to get eligible addresses")
        }
    }
}
