// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Domain routes about the voting token's delegates.
//!
//! The first route that answers a QUESTION rather than proxying a chain primitive, and the reason
//! is cost. Building the delegate directory means scanning every `DelegateChanged` the token has
//! ever emitted and then reading each delegate's current voting power. Both frontends do that in
//! the browser today: a chunked `eth_getLogs` walk from the token's deployment block, then
//! `getVotes` multicalled in batches — repeated by every client, on every page load, for a set of
//! logs this server has already indexed.
//!
//! Doing it here collapses that to one indexed read plus one `eth_call` per 200 delegates, shared
//! by every client and cached for the life of a block.
//!
//! It is also the shape that fixes what `/chain/rpc` cannot. That endpoint has to infer intent
//! from an address parameter, which is why Multicall3 could hide its real targets behind an
//! allowed `to` and why account reads had to be exempted from the allowlist entirely. This route
//! takes a token and answers one question about it; there is no calldata to inspect, and the only
//! calls it will ever make are `totalSupply()` and `getVotes(address)`.

use crate::config::CONFIG;
use crate::server::app_data::AppData;
use crate::server::models::JsonResponse;
use crate::server::rate_limit::ChainRateLimiter;

use super::chain::{admit, is_log_indexed, parse_address, too_many_requests, upstream, MULTICALL3};

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use alloy::eips::BlockNumberOrTag;
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use alloy::sol_types::{SolCall, SolEvent};
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config.service(web::scope("/members").route("/delegates", web::post().to(delegates)));
}

sol! {
    /// The event every `ERC20Votes` token emits when an account changes its delegate. `toDelegate`
    /// is indexed, so the candidate set is readable from topics alone — no log data to decode.
    event DelegateChanged(
        address indexed delegator,
        address indexed fromDelegate,
        address indexed toDelegate
    );

    function getVotes(address account) external view returns (uint256);
    function totalSupply() external view returns (uint256);

    struct Call3 {
        address target;
        bool allowFailure;
        bytes callData;
    }

    struct MulticallResult {
        bool success;
        bytes returnData;
    }

    function aggregate3(Call3[] calls) external returns (MulticallResult[] returnData);
}

/// Delegates per `aggregate3`.
///
/// The candidate set grows with every address ever delegated to, and one call carrying all of
/// them eventually exceeds the node's calldata or response limits — at which point the whole
/// directory fails rather than one batch of it.
const MULTICALL_BATCH: usize = 200;

/// Cost charged to the caller's read window.
///
/// A cache hit is free and a miss is one indexed read plus a handful of `eth_call`s, so this is
/// priced between the two rather than at either extreme.
const DELEGATES_READ_COST: usize = 8;

#[derive(Debug, Deserialize)]
pub struct DelegatesRequest {
    /// Defaults to `CRISP_VOTING_TOKEN`. Present so one server can answer for more than one app's
    /// token — the bound is which tokens it INDEXES, checked below, not an allowlist.
    #[serde(default)]
    pub token: Option<String>,
    /// The block the caller needs the history to reach back to — the token's deployment block.
    ///
    /// The server cannot know it: coverage records where THIS server started indexing, which on a
    /// deployment with no backfill is long after the token shipped. Without this the route would
    /// answer from partial history and look authoritative doing so, dropping every delegate whose
    /// last `DelegateChanged` predates the index. The caller knows the number, so the caller
    /// supplies it and the server refuses what it cannot cover.
    #[serde(default)]
    pub from_block: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DelegateEntry {
    pub address: String,
    /// A `uint256` as a decimal string: JSON numbers cannot carry it, and every consumer parses
    /// it into a bigint anyway.
    pub voting_power: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DelegatesResponse {
    pub token: String,
    /// The block every voting-power read was pinned to.
    ///
    /// Pinned, not `latest`: left unpinned, each batch resolves against whatever head it happens
    /// to hit, so a delegation landing mid-scan is counted in one batch and not another, and the
    /// percentages stop summing.
    pub block: u64,
    /// The first block indexed for this token, and the last block applied. Together they say what
    /// the candidate scan actually covered, so a client can tell a complete answer from one taken
    /// while the indexer is still catching up — the failure mode a typed route has to expose that
    /// a raw `eth_getLogs` proxy does not.
    pub indexed_from: u64,
    pub indexed_head: u64,
    pub total_supply: String,
    pub delegates: Vec<DelegateEntry>,
}

/// One computed directory, kept until the chain moves past the block it was computed at.
struct Cached {
    block: u64,
    response: DelegatesResponse,
}

/// Per-token cache.
///
/// The whole point of moving this server-side: within one block the answer is identical for every
/// caller, so the first request pays for it and the rest are a map lookup. Keyed by lowercased
/// token address; bounded by how many tokens this server indexes, which is a handful.
static CACHE: once_cell::sync::Lazy<RwLock<HashMap<String, Cached>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

/// The delegate directory for a voting token: every address ever delegated to that still holds
/// voting power, ranked, with the token's total supply for percentages.
async fn delegates(
    http_request: HttpRequest,
    request: web::Json<DelegatesRequest>,
    store: web::Data<AppData>,
    limiter: web::Data<ChainRateLimiter>,
) -> impl Responder {
    if let Err((caller, cost)) = admit(&http_request, &limiter, DELEGATES_READ_COST) {
        return too_many_requests(&caller, cost, "/members/delegates");
    }

    let requested = request
        .token
        .clone()
        .or_else(|| CONFIG.crisp_voting_token.clone())
        .unwrap_or_default();

    let Some(token) = parse_address(&requested) else {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: format!("Invalid token address: {requested}"),
        });
    };
    let token_key = token.to_string().to_lowercase();

    // Not a permission check: this route can only ANSWER from the log index, so a token whose
    // logs are not indexed has no answer here at all. Refusing plainly lets the client fall back
    // to scanning for itself, which is what it does today.
    if !is_log_indexed(&token_key) {
        return not_indexed(&token);
    }

    let repo = store.logs();
    let (Ok(Some(indexed_from)), Ok(Some(indexed_head))) =
        (repo.coverage(&token_key).await, repo.indexed_head().await)
    else {
        return not_indexed(&token);
    };

    // Not clamped, and not answered partially: an answer built from history that starts after the
    // token did is missing delegates, and nothing in the response would distinguish it from a
    // complete one. The same rule `/chain/logs` follows — a query reaching past what is covered
    // goes back to the caller rather than being served short.
    if let Some(required_from) = request.from_block {
        if indexed_from > required_from {
            return HttpResponse::NotFound().json(JsonResponse {
                response: format!(
                    "Delegate history for {token} is indexed only from block {indexed_from}, \
                     but the caller needs it from {required_from}"
                ),
            });
        }
    }

    let provider = match upstream().await {
        Ok(p) => p,
        Err(e) => {
            error!("members/delegates: provider unavailable: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    let block = match provider.get_block_number().await {
        Ok(number) => number,
        Err(e) => {
            error!("members/delegates: could not read the head: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    if let Some(hit) = CACHE.read().await.get(&token_key) {
        if hit.block == block {
            return HttpResponse::Ok().json(hit.response.clone());
        }
    }

    let candidates = match candidate_delegates(&store, &token_key, indexed_from, indexed_head).await
    {
        Ok(addresses) => addresses,
        Err(e) => {
            error!("members/delegates: reading the log index failed: {e}");
            return HttpResponse::InternalServerError().json(JsonResponse {
                response: "Failed to read the delegate history".to_string(),
            });
        }
    };

    let response = match build_directory(provider, token, block, &candidates).await {
        Ok(mut built) => {
            built.indexed_from = indexed_from;
            built.indexed_head = indexed_head;
            built
        }
        Err(e) => {
            error!("members/delegates: reading voting power failed: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Failed to read voting power".to_string(),
            });
        }
    };

    CACHE.write().await.insert(
        token_key,
        Cached {
            block,
            response: response.clone(),
        },
    );

    HttpResponse::Ok().json(response)
}

fn not_indexed(token: &Address) -> HttpResponse {
    HttpResponse::NotFound().json(JsonResponse {
        response: format!(
            "Delegate history for {token} is not indexed by this server; add it to INDEX_LOG_CONTRACTS"
        ),
    })
}

/// Every address the token has ever delegated TO, in first-seen order.
///
/// Read from topics: `toDelegate` is indexed, so no log data has to be decoded, and an address
/// that has since delegated away is still a candidate — whether it currently holds power is
/// decided by `getVotes` below, not by the last event about it.
async fn candidate_delegates(
    store: &web::Data<AppData>,
    token_key: &str,
    from: u64,
    to: u64,
) -> eyre::Result<Vec<Address>> {
    let topic0 = format!("{:#x}", DelegateChanged::SIGNATURE_HASH);
    let logs = store
        .logs()
        .query(token_key, from, to, &[Some(topic0), None, None, None])
        .await?;

    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();

    for log in logs {
        // topics[3] is `toDelegate`: [signature, delegator, fromDelegate, toDelegate].
        let Some(topic) = log.topics.get(3) else {
            continue;
        };
        let Some(address) = address_from_topic(topic) else {
            continue;
        };
        // The zero address is what `delegate(address(0))` records — an undelegation, not a
        // delegate. It can never hold voting power.
        if address == Address::ZERO {
            continue;
        }
        if seen.insert(address) {
            candidates.push(address);
        }
    }

    Ok(candidates)
}

/// A 32-byte topic word carrying a left-padded address.
fn address_from_topic(topic: &str) -> Option<Address> {
    let hex = topic.trim().trim_start_matches("0x");
    if hex.len() != 64 {
        return None;
    }
    parse_address(&hex[24..])
}

/// Total supply plus each candidate's voting power at `block`, zeros dropped, ranked.
async fn build_directory(
    provider: &alloy::providers::DynProvider,
    token: Address,
    block: u64,
    candidates: &[Address],
) -> eyre::Result<DelegatesResponse> {
    let supply_call = totalSupplyCall {};
    let supply_raw = provider
        .call(
            TransactionRequest::default()
                .with_to(token)
                .with_input(Bytes::from(supply_call.abi_encode())),
        )
        .block(BlockNumberOrTag::Number(block).into())
        .await?;
    let total_supply = totalSupplyCall::abi_decode_returns(&supply_raw)?;

    let mut delegates = Vec::with_capacity(candidates.len());

    for chunk in candidates.chunks(MULTICALL_BATCH) {
        let calls = chunk
            .iter()
            .map(|account| Call3 {
                target: token,
                // Per-call failure rather than a reverting batch: one token that does not
                // implement `getVotes` for one account must not discard the whole directory.
                allowFailure: true,
                callData: Bytes::from(getVotesCall { account: *account }.abi_encode()),
            })
            .collect::<Vec<_>>();

        let raw = provider
            .call(
                TransactionRequest::default()
                    .with_to(MULTICALL3)
                    .with_input(Bytes::from(
                        aggregate3Call {
                            calls: calls.clone(),
                        }
                        .abi_encode(),
                    )),
            )
            .block(BlockNumberOrTag::Number(block).into())
            .await?;

        let results = aggregate3Call::abi_decode_returns(&raw)?;

        for (account, result) in chunk.iter().zip(results) {
            if !result.success {
                continue;
            }
            let Ok(power) = getVotesCall::abi_decode_returns(&result.returnData) else {
                continue;
            };
            if power == U256::ZERO {
                continue;
            }
            delegates.push(DelegateEntry {
                address: account.to_string(),
                voting_power: power.to_string(),
            });
        }
    }

    // Ranked here rather than in the client: every consumer wants the same order, and sorting
    // decimal strings on the client means parsing them all first.
    delegates.sort_by(|a, b| {
        let left: U256 = a.voting_power.parse().unwrap_or(U256::ZERO);
        let right: U256 = b.voting_power.parse().unwrap_or(U256::ZERO);
        right.cmp(&left)
    });

    Ok(DelegatesResponse {
        token: token.to_string(),
        block,
        indexed_from: 0,
        indexed_head: 0,
        total_supply: total_supply.to_string(),
        delegates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::database::SledDB;
    use crate::server::log_repo::StoredLog;
    use e3_sdk::indexer::SharedStore;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// A store in a directory of its own, removed when the guard drops.
    struct TempStore {
        path: std::path::PathBuf,
        data: web::Data<AppData>,
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn temp_store() -> TempStore {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "crisp-members-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let db = SharedStore::new(Arc::new(tokio::sync::RwLock::new(
            SledDB::new(path.to_str().expect("temp path is valid utf-8")).expect("opening sled"),
        )));
        TempStore {
            path,
            data: web::Data::new(AppData::new(db)),
        }
    }

    fn topic_for(address: Address) -> String {
        format!("0x{:0>64}", format!("{:x}", address))
    }

    fn delegate_changed(token: Address, to_delegate: Address, block: u64, index: u64) -> StoredLog {
        StoredLog {
            removed: false,
            address: token.to_string().to_lowercase(),
            topics: vec![
                format!("{:#x}", DelegateChanged::SIGNATURE_HASH),
                topic_for(Address::repeat_byte(0xaa)),
                topic_for(Address::ZERO),
                topic_for(to_delegate),
            ],
            data: "0x".to_string(),
            block_number: block,
            transaction_hash: None,
            log_index: index,
            block_hash: None,
            transaction_index: None,
        }
    }

    #[actix_web::test]
    async fn candidates_are_deduplicated_and_kept_in_first_seen_order() {
        let store = temp_store();
        let token = Address::repeat_byte(0x11);
        let first = Address::repeat_byte(0x22);
        let second = Address::repeat_byte(0x33);

        let mut logs = store.data.logs();
        logs.append(delegate_changed(token, first, 100, 0))
            .await
            .unwrap();
        logs.append(delegate_changed(token, second, 101, 0))
            .await
            .unwrap();
        // The same delegate again, and an undelegation to the zero address.
        logs.append(delegate_changed(token, first, 102, 0))
            .await
            .unwrap();
        logs.append(delegate_changed(token, Address::ZERO, 103, 0))
            .await
            .unwrap();

        let token_key = token.to_string().to_lowercase();
        let found = candidate_delegates(&store.data, &token_key, 0, 200)
            .await
            .unwrap();

        assert_eq!(found, vec![first, second]);
    }

    #[actix_web::test]
    async fn a_delegate_outside_the_scanned_range_is_not_a_candidate() {
        let store = temp_store();
        let token = Address::repeat_byte(0x11);
        let inside = Address::repeat_byte(0x22);
        let outside = Address::repeat_byte(0x33);

        let mut logs = store.data.logs();
        logs.append(delegate_changed(token, inside, 50, 0))
            .await
            .unwrap();
        logs.append(delegate_changed(token, outside, 5_000, 0))
            .await
            .unwrap();

        let token_key = token.to_string().to_lowercase();
        let found = candidate_delegates(&store.data, &token_key, 0, 100)
            .await
            .unwrap();

        assert_eq!(found, vec![inside]);
    }

    #[actix_web::test]
    async fn another_contracts_events_are_not_mixed_in() {
        let store = temp_store();
        let token = Address::repeat_byte(0x11);
        let other_token = Address::repeat_byte(0x99);
        let ours = Address::repeat_byte(0x22);
        let theirs = Address::repeat_byte(0x33);

        let mut logs = store.data.logs();
        logs.append(delegate_changed(token, ours, 10, 0))
            .await
            .unwrap();
        logs.append(delegate_changed(other_token, theirs, 11, 0))
            .await
            .unwrap();

        let token_key = token.to_string().to_lowercase();
        let found = candidate_delegates(&store.data, &token_key, 0, 100)
            .await
            .unwrap();

        assert_eq!(found, vec![ours]);
    }

    #[test]
    fn an_address_is_read_out_of_its_padded_topic_word() {
        let topic = "0x000000000000000000000000cA11bde05977b3631167028862bE2a173976CA11";
        assert_eq!(address_from_topic(topic), Some(MULTICALL3));
    }

    #[test]
    fn a_malformed_topic_is_skipped_rather_than_guessed_at() {
        assert_eq!(address_from_topic("0x1234"), None);
        assert_eq!(address_from_topic(""), None);
        // A word the right length but not hex is not an address either.
        assert_eq!(address_from_topic(&format!("0x{}", "z".repeat(64))), None);
    }

    #[test]
    fn the_delegate_changed_signature_matches_the_erc20votes_event() {
        // keccak256("DelegateChanged(address,address,address)") — the topic both frontends filter
        // on today, so an answer from this route covers the same events their scan does.
        assert_eq!(
            format!("{:#x}", DelegateChanged::SIGNATURE_HASH),
            "0x3134e8a2e6d97e929a7e54011ea5485d7d196dd5f0ba4d4ef95803e8e3fc257f"
        );
    }
}
