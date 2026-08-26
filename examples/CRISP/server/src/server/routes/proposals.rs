// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! The plugin's proposal list, from its `ProposalCreated` history.
//!
//! The second-heaviest read in both frontends after the delegate directory, and the same shape of
//! waste: the proposal list walks the plugin's logs from its deployment block on every page load,
//! and each proposal page then walks them AGAIN, filtered to one id, to recover the metadata URI.
//! Every client repeats both against logs this server already watches.
//!
//! Cheaper to serve than delegates, too. A directory's voting power changes with every delegation,
//! so it has to be recomputed per block; a proposal's creation event is immutable. The list only
//! ever grows, so it is extended to the head and never rebuilt.

use crate::server::app_data::AppData;
use crate::server::models::JsonResponse;
use crate::server::rate_limit::ChainRateLimiter;

use super::chain::{admit, is_allowed, parse_address, too_many_requests, upstream};
use super::scan::{coverage_for, scan_logs, Target};

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use alloy::primitives::Address;
use alloy::providers::Provider;
use alloy::sol;
use alloy::sol_types::SolEvent;
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config.service(web::scope("/proposals").route("", web::post().to(proposals)));
}

sol! {
    struct Action {
        address to;
        uint256 value;
        bytes data;
    }

    /// Aragon's `ProposalCreated`. `metadata` is the IPFS URI as bytes — the one field the
    /// proposal page cannot get from the contract's `getProposal`, which is why it scans logs at
    /// all.
    event ProposalCreated(
        uint256 indexed proposalId,
        address indexed creator,
        uint64 startDate,
        uint64 endDate,
        bytes metadata,
        Action[] actions,
        uint256 allowFailureMap
    );
}

/// Cost charged to the caller's read window: a cache hit is free, a cold scan is a handful of
/// windows.
const PROPOSALS_READ_COST: usize = 4;

#[derive(Debug, Deserialize)]
pub struct ProposalsRequest {
    /// The plugin whose proposals these are. Required, and must be a contract this server is
    /// configured to serve — this route is about a KNOWN plugin, not a way to scan any address.
    pub plugin: String,
    /// The block the caller needs history from — the plugin's deployment block. As with
    /// `/members/delegates`, the server cannot know it: its own coverage begins wherever it
    /// started indexing.
    #[serde(default)]
    pub from_block: Option<u64>,
    /// Narrow the answer to one proposal, for the per-proposal page. Filtered here rather than by
    /// the caller so the response stays small.
    #[serde(default)]
    pub proposal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    /// Decimal string: a `uint256` proposal id does not survive a JSON number.
    pub proposal_id: String,
    pub creator: String,
    pub start_date: u64,
    pub end_date: u64,
    /// The raw `metadata` bytes, hex-encoded. The client decodes it to the IPFS URI, as it does
    /// with the log today.
    pub metadata: String,
    pub block: u64,
    pub transaction_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalsResponse {
    pub plugin: String,
    /// The range actually scanned. `scanned_from` later than the plugin's deployment means
    /// proposals are missing, and nothing else in the response would show it.
    pub scanned_from: u64,
    pub scanned_to: u64,
    /// How far this server's own index reaches. Below `scanned_to` means the rest came from its
    /// upstream provider.
    pub indexed_head: u64,
    pub proposals: Vec<Proposal>,
}

/// Proposals accumulate and never change, so unlike the delegate directory this is extended to
/// the head rather than recomputed per block.
#[derive(Default)]
struct PluginCache {
    scanned: Option<(u64, u64)>,
    proposals: Vec<Proposal>,
}

static CACHE: once_cell::sync::Lazy<RwLock<HashMap<String, PluginCache>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

/// One in-flight scan per plugin, so a cold cache is not a thundering herd. See the same pattern
/// in `members`.
static SCANS: once_cell::sync::Lazy<
    RwLock<HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
> = once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

async fn scan_lock(plugin_key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    if let Some(existing) = SCANS.read().await.get(plugin_key) {
        return existing.clone();
    }
    SCANS
        .write()
        .await
        .entry(plugin_key.to_string())
        .or_default()
        .clone()
}

/// The cached list when it covers `[scan_from, block]`.
async fn cached_proposals(plugin_key: &str, scan_from: u64, block: u64) -> Option<Vec<Proposal>> {
    let cache = CACHE.read().await;
    let entry = cache.get(plugin_key)?;
    let (from, to) = entry.scanned?;
    (from <= scan_from && to >= block).then(|| entry.proposals.clone())
}

async fn proposals(
    http_request: HttpRequest,
    request: web::Json<ProposalsRequest>,
    store: web::Data<AppData>,
    limiter: web::Data<ChainRateLimiter>,
) -> impl Responder {
    if let Err((caller, cost)) = admit(&http_request, &limiter, PROPOSALS_READ_COST) {
        return too_many_requests(&caller, cost, "/proposals");
    }

    let Some(plugin) = parse_address(&request.plugin) else {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: format!("Invalid plugin address: {}", request.plugin),
        });
    };

    // The one bound this route needs. Without it, a `from_block` far enough back turns it into a
    // windowed log scanner for any contract on the chain — the general-purpose primitive these
    // typed routes exist to replace.
    if !is_allowed(&plugin) {
        return HttpResponse::NotFound().json(JsonResponse {
            response: format!("Plugin {plugin} is not served by this indexer"),
        });
    }

    let plugin_key = plugin.to_string().to_lowercase();
    let indexed = coverage_for(&store, &plugin_key).await;
    let indexed_head = indexed.map(|(_, head)| head).unwrap_or(0);

    let Some(scan_from) = request.from_block.or(indexed.map(|(from, _)| from)) else {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: format!(
                "from_block is required for {plugin}: this server does not index its logs, so it \
                 has no deployment block to scan from"
            ),
        });
    };

    let provider = match upstream().await {
        Ok(p) => p,
        Err(e) => {
            error!("proposals: provider unavailable: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    let block = match provider.get_block_number().await {
        Ok(number) => number,
        Err(e) => {
            error!("proposals: could not read the head: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    if let Some(hit) = cached_proposals(&plugin_key, scan_from, block).await {
        return respond(&request, plugin, scan_from, block, indexed_head, hit);
    }

    let lock = scan_lock(&plugin_key).await;
    let _scanning = lock.lock().await;

    if let Some(hit) = cached_proposals(&plugin_key, scan_from, block).await {
        return respond(&request, plugin, scan_from, block, indexed_head, hit);
    }

    // Extend what is already held rather than rescanning it: a creation event is immutable, so
    // the only thing that can have changed is that there are more of them.
    let reusable = CACHE.read().await.get(&plugin_key).and_then(|entry| {
        entry
            .scanned
            .filter(|(from, to)| *from <= scan_from && *to <= block)
            .map(|(from, to)| (from, to, entry.proposals.clone()))
    });

    let (known, scan_start, scanned_from) = match reusable {
        Some((from, to, known)) => (known, to + 1, from),
        None => (Vec::new(), scan_from, scan_from),
    };

    let logs = match scan_logs(
        &store,
        provider,
        &Target {
            address: plugin,
            key: &plugin_key,
            topic0: ProposalCreated::SIGNATURE_HASH,
            indexed,
        },
        scan_start,
        block,
    )
    .await
    {
        Ok(found) => found,
        Err(e) => {
            error!("proposals: scanning ProposalCreated failed: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Failed to read the proposal history".to_string(),
            });
        }
    };

    let mut all = known;
    let mut seen: std::collections::HashSet<String> =
        all.iter().map(|p| p.proposal_id.clone()).collect();

    for log in logs {
        // A log that will not decode is skipped, not fatal: one unreadable event should cost that
        // proposal, not the whole list. It happens for real — an older plugin on the same address
        // with a different event shape would land here.
        let Ok(decoded) = ProposalCreated::decode_raw_log(log.topics.iter().copied(), &log.data)
        else {
            continue;
        };

        let proposal_id = decoded.proposalId.to_string();
        if !seen.insert(proposal_id.clone()) {
            continue;
        }

        all.push(Proposal {
            proposal_id,
            creator: decoded.creator.to_string(),
            start_date: decoded.startDate,
            end_date: decoded.endDate,
            metadata: format!("0x{}", hex::encode(&decoded.metadata)),
            block: log.block_number,
            transaction_hash: log.transaction_hash,
        });
    }

    // Newest first: every consumer renders them that way, and ordering here means the client does
    // not re-sort a list it just parsed.
    all.sort_by(|a, b| b.block.cmp(&a.block));

    CACHE.write().await.insert(
        plugin_key,
        PluginCache {
            scanned: Some((scanned_from, block)),
            proposals: all.clone(),
        },
    );

    respond(&request, plugin, scanned_from, block, indexed_head, all)
}

fn respond(
    request: &ProposalsRequest,
    plugin: Address,
    scanned_from: u64,
    scanned_to: u64,
    indexed_head: u64,
    proposals: Vec<Proposal>,
) -> HttpResponse {
    let proposals = match &request.proposal_id {
        Some(wanted) => proposals
            .into_iter()
            .filter(|proposal| &proposal.proposal_id == wanted)
            .collect(),
        None => proposals,
    };

    HttpResponse::Ok().json(ProposalsResponse {
        plugin: plugin.to_string(),
        scanned_from,
        scanned_to,
        indexed_head,
        proposals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proposal_created_signature_matches_the_event_both_frontends_filter_on() {
        // keccak256 of the Aragon signature both clients pass to `getLogs`, cross-checked
        // against viem's `toEventSelector` for the same signature. If this drifts, the route
        // returns an empty list rather than failing — silence is the whole reason to pin it.
        assert_eq!(
            format!("{:#x}", ProposalCreated::SIGNATURE_HASH),
            "0xa6c1f8f4276dc3f243459e13b557c84e8f4e90b2e09070bad5f6909cee687c92"
        );
    }

    #[test]
    fn one_proposal_can_be_asked_for_by_id() {
        let request = ProposalsRequest {
            plugin: String::new(),
            from_block: None,
            proposal_id: Some("2".to_string()),
        };
        let all = vec![
            Proposal {
                proposal_id: "1".to_string(),
                creator: String::new(),
                start_date: 0,
                end_date: 0,
                metadata: String::new(),
                block: 1,
                transaction_hash: None,
            },
            Proposal {
                proposal_id: "2".to_string(),
                creator: String::new(),
                start_date: 0,
                end_date: 0,
                metadata: String::new(),
                block: 2,
                transaction_hash: None,
            },
        ];

        let response = respond(&request, Address::ZERO, 0, 0, 0, all);
        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
    }
}
