// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Chain access for clients that have no RPC provider of their own.
//!
//! A browser app built on CRISP otherwise needs its own hosted-provider key just to read the
//! contracts it already talks to through this server. These routes close that gap, and they are
//! deliberately NOT a general purpose JSON-RPC proxy:
//!
//! - Every method that names an address is checked against an allowlist (`INDEX_CONTRACTS` plus
//!   the contracts this server is configured against), so the server cannot be turned into free
//!   RPC for the rest of the chain. The data itself is public — the allowlist bounds cost and
//!   abuse, not disclosure. The check fails closed in both directions: an unknown parameter shape
//!   is refused, and so is a request that omits the address a method could have carried (an
//!   `eth_call` with no `to` is arbitrary EVM execution; an `eth_getLogs` with no `address` is
//!   every log on the chain). The few methods that name no address at all are bounded by shape
//!   instead — no full transaction bodies, and a capped `feeHistory`.
//! - Only reads. There is no path here that can send a transaction; writes stay with the user's
//!   wallet, which brings its own transport.
//! - Log queries are windowed server-side, so a caller may ask for the whole history of a contract
//!   in one request without knowing the provider's `eth_getLogs` range cap. Working around that
//!   cap in the browser is exactly the chunked-scan code this replaces.

use crate::config::CONFIG;
use crate::server::app_data::AppData;
use crate::server::models::JsonResponse;
use crate::server::read_cache;

use actix_web::{web, HttpResponse, Responder};
use alloy::eips::BlockNumberOrTag;
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, B256};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::client::RpcClient;
use alloy::transports::http::Http;
use alloy::rpc::types::{Filter, TransactionRequest};
use log::error;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Upper bound on a single `eth_getLogs` window, matching the indexer's default. The server
/// splits a caller's range into windows of this size.
const LOG_WINDOW: u64 = 2_000;

/// Cap on how many calls one `/chain/read` request may batch, so a single request cannot fan out
/// into unbounded provider load.
const MAX_BATCH: usize = 64;

/// Cap on how many windows one log query may expand into.
///
/// Windowing is what lets a caller ignore the provider's range cap, but it also means a single
/// request for "genesis to head" would become thousands of sequential upstream calls. Refusing
/// with a clear bound is better than accepting a request that ties up a connection for minutes:
/// callers know their contract's deployment block, and asking from there is the intended usage.
const MAX_LOG_WINDOWS: u64 = 500;

/// Cap on how many calls one JSON-RPC batch may carry.
///
/// A batch is executed sequentially, so an unbounded array is an unbounded number of upstream
/// requests held open on one connection — the same fan-out `/chain/read` already bounds.
const MAX_RPC_BATCH: usize = 64;

/// How long to wait on the upstream provider before giving up.
///
/// Without one, reqwest waits forever: a provider that accepts connections and then stalls would
/// pin a worker per request until the process is restarted.
const UPSTREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// One HTTP client for the process, rather than one per request.
///
/// `Client` owns a connection pool; building it per call threw the pool away every time and paid
/// a fresh TLS handshake for each upstream request.
static HTTP: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(UPSTREAM_TIMEOUT)
        .build()
        .expect("building the upstream HTTP client cannot fail with a timeout as its only option")
});

/// One alloy provider for the process, built on the same timeout-bounded client as [`HTTP`].
///
/// Every typed route used to call `ProviderBuilder::new().connect(...)` per request, which threw
/// away the connection pool each time and — more importantly — inherited no request timeout, so a
/// provider that accepted a connection and then stalled pinned a worker until restart. The
/// JSON-RPC forward path was fixed first; these are the rest of them.
static PROVIDER: tokio::sync::OnceCell<DynProvider> = tokio::sync::OnceCell::const_new();

async fn upstream() -> eyre::Result<&'static DynProvider> {
    PROVIDER
        .get_or_try_init(|| async {
            let url: reqwest::Url = CONFIG
                .http_rpc_url
                .parse()
                .map_err(|e| eyre::eyre!("HTTP_RPC_URL is not a valid URL: {e}"))?;

            let transport = Http::with_client(HTTP.clone(), url);
            Ok(ProviderBuilder::new()
                .connect_client(RpcClient::new(transport, false))
                .erased())
        })
        .await
}

/// Whether a range is small enough to serve, given the window size.
fn windows_for(from: u64, to: u64) -> u64 {
    if from > to {
        return 0;
    }
    (to - from) / LOG_WINDOW + 1
}

pub fn setup_routes(config: &mut web::ServiceConfig) {
    config.service(
        web::scope("/chain")
            .route("/rpc", web::post().to(rpc))
            .route("/head", web::post().to(head))
            .route("/read", web::post().to(read))
            .route("/logs", web::post().to(logs))
            .route("/block-at-timestamp", web::post().to(block_at_timestamp))
            .route("/stats", web::post().to(stats)),
    );
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub call_hits: u64,
    pub call_misses: u64,
    pub head_hits: u64,
    pub head_misses: u64,
    pub log_index_hits: u64,
    pub log_upstream: u64,
    /// Upstream requests avoided: every cache hit and every log query the index answered.
    pub upstream_calls_saved: u64,
}

/// How much upstream traffic this server is absorbing.
///
/// Exposed because "the indexer saves RPC calls" is a claim that should be checkable against a
/// running deployment rather than taken on trust.
async fn stats() -> impl Responder {
    let counters = read_cache::COUNTERS.read().await;

    HttpResponse::Ok().json(StatsResponse {
        call_hits: counters.call_hits,
        call_misses: counters.call_misses,
        head_hits: counters.head_hits,
        head_misses: counters.head_misses,
        log_index_hits: counters.log_index_hits,
        log_upstream: counters.log_upstream,
        upstream_calls_saved: counters.call_hits + counters.head_hits + counters.log_index_hits,
    })
}

/// JSON-RPC methods this endpoint will forward.
///
/// An allowlist rather than a denylist of writes: a new method appearing in a future provider or
/// client should be unreachable until someone decides it belongs here, and the cost of that
/// choice is a clear error instead of an unintended capability. Everything here is a read —
/// notably absent are `eth_sendRawTransaction` and `eth_sendTransaction`, because transactions
/// are signed and broadcast by the user's own wallet, which brings its own transport.
const ALLOWED_RPC_METHODS: &[&str] = &[
    "eth_call",
    "eth_getLogs",
    "eth_blockNumber",
    "eth_chainId",
    "eth_getBlockByNumber",
    "eth_getBlockByHash",
    "eth_getCode",
    "eth_getStorageAt",
    "eth_getBalance",
    "eth_getTransactionByHash",
    "eth_getTransactionReceipt",
    "eth_getTransactionCount",
    "eth_estimateGas",
    "eth_gasPrice",
    "eth_maxPriorityFeePerGas",
    "eth_feeHistory",
    "net_version",
    "web3_clientVersion",
];

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    // `jsonrpc` is accepted and ignored: serde skips unknown fields, and the version is not
    // something this endpoint varies on.
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

fn rpc_error(id: Option<serde_json::Value>, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

/// The cache key for an `eth_call`, or `None` when the shape is not one we can key on.
///
/// A call carrying `from`, `value` or a gas field is not keyed: those can change the result, and
/// a key that ignores them would serve one caller's answer to another.
fn call_cache_key(params: &serde_json::Value) -> Option<(String, String, Option<u64>)> {
    // A state override object in the third position lets the caller choose the bytes the node
    // returns. It is forwarded verbatim, so a key that ignores it would let one caller pick what
    // every other caller is served for the rest of the block.
    if params.get(2).is_some_and(|v| !v.is_null()) {
        return None;
    }

    let tx = params.get(0)?.as_object()?;

    // An allowlist of the fields the key accounts for, not a denylist of the ones known to break
    // it: any field this endpoint does not understand may change the result, and a new one
    // appearing in a future client must not silently become uncounted.
    if tx
        .keys()
        .any(|field| !matches!(field.as_str(), "to" | "data" | "input"))
    {
        return None;
    }

    let address = tx.get("to")?.as_str()?.to_string();
    let data = tx
        .get("data")
        .or_else(|| tx.get("input"))?
        .as_str()?
        .to_string();

    let block = match params.get(1) {
        // Absent or null: `latest` by JSON-RPC default.
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(tag)) => match tag.as_str() {
            "latest" => None,
            // Anything that is not a plain `latest` or a concrete height (pending, safe,
            // finalized) is left uncached rather than guessed at.
            hex if hex.starts_with("0x") => {
                Some(u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok()?)
            }
            _ => return None,
        },
        // The EIP-1898 object form (`{"blockNumber":…}` / `{"blockHash":…}`). Reading it with
        // `as_str` yielded `None`, which is the key for `latest` — so a result the node computed
        // at an arbitrary historical block was filed as the current one, and any caller could
        // choose what every other caller saw. Not keyed at all rather than keyed wrongly.
        Some(_) => return None,
    };

    Some((address, data, block))
}

/// Which addresses a call is scoped to, so they can be checked against the allowlist.
///
/// The distinction that matters is between a method that carries NO address by construction and
/// one whose address this function failed to find: the first is a global read that the allowlist
/// cannot bound at all, the second is an unrecognised shape. Returning an empty vec for both is
/// what made the allowlist decorative — an `eth_call` with no `to`, or an `eth_getLogs` with no
/// `address`, skipped the check entirely and was forwarded verbatim.
enum Scope {
    /// Check every one of these against the allowlist before forwarding.
    Addresses(Vec<String>),
    /// The method takes no address; nothing to check, and nothing this endpoint can bound by
    /// address either.
    Global,
    /// The method should be address-scoped but this request is not. Refuse.
    Unscoped(&'static str),
}

/// Pull the address (or addresses) a request is scoped to out of its parameter list.
///
/// Fails closed: every method that CAN name an address must name one, and any shape this does not
/// recognise is a refusal rather than a pass.
fn requested_addresses(method: &str, params: &serde_json::Value) -> Scope {
    // Methods whose address is a bare string in the first position.
    let positional = matches!(
        method,
        "eth_getCode" | "eth_getBalance" | "eth_getStorageAt" | "eth_getTransactionCount"
    );

    if positional {
        return match params.get(0) {
            Some(serde_json::Value::String(one)) => Scope::Addresses(vec![one.clone()]),
            _ => Scope::Unscoped("this method requires an address in the first parameter"),
        };
    }

    let field = match method {
        // An `eth_call` with no `to` is a contract-creation simulation: the caller supplies
        // initcode that runs arbitrary EVM, which is a read of any contract on the chain by
        // another name. There is no address to check, so there is nothing to allow.
        "eth_call" | "eth_estimateGas" => "to",
        // An absent, null or empty `address` means "every address" to a node. Serving that would
        // return the whole chain's logs through an endpoint whose stated bound is an allowlist.
        "eth_getLogs" => "address",
        _ => return Scope::Global,
    };

    let Some(first) = params.get(0) else {
        return Scope::Unscoped("this method requires a filter or call object");
    };

    match first.get(field) {
        Some(serde_json::Value::String(one)) => Scope::Addresses(vec![one.clone()]),
        Some(serde_json::Value::Array(many)) if !many.is_empty() => {
            let mut addresses = Vec::with_capacity(many.len());
            for entry in many {
                match entry.as_str() {
                    Some(one) => addresses.push(one.to_string()),
                    None => return Scope::Unscoped("addresses must be strings"),
                }
            }
            Scope::Addresses(addresses)
        }
        _ => Scope::Unscoped("this method requires an explicit address"),
    }
}

/// Cap on `eth_feeHistory`'s block count, which is otherwise a caller-chosen fan-out.
const MAX_FEE_HISTORY_BLOCKS: u64 = 128;

/// Reject the shapes of an address-less method that would return an unbounded response.
///
/// These methods cannot be bounded by the allowlist — there is no address in them — so the only
/// remaining lever is refusing the expensive variants. A block request with full transaction
/// bodies is the largest single response a node will produce, and nothing in this server's
/// intended use needs one.
fn global_request_is_too_broad(method: &str, params: &serde_json::Value) -> Option<&'static str> {
    match method {
        "eth_getBlockByNumber" | "eth_getBlockByHash" => params
            .get(1)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            .then_some("full transaction bodies are not served; pass false"),
        "eth_feeHistory" => {
            let count = params.get(0).and_then(|v| match v {
                serde_json::Value::String(hex) => {
                    u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
                }
                serde_json::Value::Number(n) => n.as_u64(),
                _ => None,
            })?;
            (count > MAX_FEE_HISTORY_BLOCKS).then_some("feeHistory block count is too large")
        }
        _ => None,
    }
}

/// A read-only, allowlisted JSON-RPC endpoint.
///
/// This exists so a browser client can point a standard Ethereum library at the CRISP server and
/// keep working, rather than every caller reimplementing its reads against the typed routes below.
/// The typed routes remain the nicer interface for anything new; this one is what makes dropping a
/// hosted-provider key a configuration change instead of a rewrite.
///
/// `eth_getLogs` is special-cased: the range is split into windows here, so a caller may ask for a
/// contract's whole history without knowing the upstream provider's cap.
async fn rpc(body: web::Json<serde_json::Value>, store: web::Data<AppData>) -> impl Responder {
    // A JSON-RPC endpoint must accept a batch as a top-level array, and viem sends one whenever
    // `batch: true` is set — which both of our clients do. Rejecting arrays at the extractor made
    // every batched request a 400 that no JSON-RPC client knows how to read.
    match body.into_inner() {
        serde_json::Value::Array(entries) => {
            // An empty batch is an Invalid Request per the spec, not an empty result.
            if entries.is_empty() {
                return HttpResponse::Ok().json(rpc_error(
                    None,
                    -32600,
                    "Invalid request: empty batch",
                ));
            }

            if entries.len() > MAX_RPC_BATCH {
                return HttpResponse::Ok().json(rpc_error(
                    None,
                    -32600,
                    &format!("At most {MAX_RPC_BATCH} calls per batch"),
                ));
            }

            let mut responses = Vec::with_capacity(entries.len());
            for entry in entries {
                responses.push(handle_rpc_call(entry, &store).await);
            }
            HttpResponse::Ok().json(responses)
        }
        single => HttpResponse::Ok().json(handle_rpc_call(single, &store).await),
    }
}

/// One JSON-RPC call, returning the response object rather than an HTTP response so the batch
/// path above can collect them.
async fn handle_rpc_call(
    entry: serde_json::Value,
    store: &web::Data<AppData>,
) -> serde_json::Value {
    let id = entry.get("id").cloned();

    let request: RpcRequest = match serde_json::from_value(entry) {
        Ok(parsed) => parsed,
        Err(e) => return rpc_error(id, -32600, &format!("Invalid request: {e}")),
    };

    if !ALLOWED_RPC_METHODS.contains(&request.method.as_str()) {
        return rpc_error(
            id,
            -32601,
            &format!("Method not served by this indexer: {}", request.method),
        );
    }

    match requested_addresses(&request.method, &request.params) {
        Scope::Addresses(addresses) => {
            for address in addresses {
                match parse_address(&address) {
                    Some(parsed) if is_allowed(&parsed) => {}
                    Some(parsed) => {
                        return rpc_error(
                            id,
                            -32602,
                            &format!("Address not served by this indexer: {parsed}"),
                        );
                    }
                    None => return rpc_error(id, -32602, "Invalid address"),
                }
            }
        }
        Scope::Unscoped(reason) => {
            return rpc_error(id, -32602, &format!("{}: {reason}", request.method));
        }
        Scope::Global => {
            // Nothing to check by address, so the bound has to come from the shape of the request
            // itself: these are the methods that can return an unbounded amount of data.
            if let Some(reason) = global_request_is_too_broad(&request.method, &request.params) {
                return rpc_error(id, -32602, reason);
            }
        }
    }

    // Constant for the life of the deployment and already in config — asking the provider for it
    // is pure waste, and viem asks on every client boot.
    if request.method == "eth_chainId" {
        return serde_json::json!({
            "jsonrpc": "2.0", "id": id, "result": format!("0x{:x}", CONFIG.chain_id),
        });
    }

    if request.method == "eth_blockNumber" {
        if let Some(number) = read_cache::head_number().await {
            read_cache::record_head(true).await;
            return serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": format!("0x{:x}", number),
            });
        }
        read_cache::record_head(false).await;
    }

    // Same per-block reasoning as `/chain/read`: this is the path the frontends take, so it is
    // where the duplication across clients actually happens.
    let cacheable_call = request.method == "eth_call";
    let call_key = cacheable_call
        .then(|| call_cache_key(&request.params))
        .flatten();
    // Read BEFORE the upstream request: if the head moves while it is in flight, the result
    // describes the older block and must not be filed under the newer one.
    let issued_at_block = read_cache::current_latest_block().await;

    if let Some((address, data, block)) = &call_key {
        if let Some(hit) = read_cache::call(address, data, *block).await {
            read_cache::record_call(true).await;
            return serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": hit,
            });
        }
        read_cache::record_call(false).await;
    }

    let client = &*HTTP;

    // Handled separately for two reasons: the index can usually answer it outright, and when it
    // cannot, forwarding a wide range verbatim would just relay the provider's own range-cap error
    // back to a caller with no way to know the cap. This is the path the frontends actually take,
    // so it is where serving from the index matters.
    if request.method == "eth_getLogs" {
        if let Some(indexed) = logs_from_index(store, &request.params).await {
            read_cache::record_logs(true).await;
            return serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": indexed,
            });
        }
        read_cache::record_logs(false).await;

        return match forward_windowed_logs(client, &request.params).await {
            Ok(logs) => serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": logs,
            }),
            // The range-cap message is the one thing a caller can act on, so it survives rather
            // than being flattened into a generic upstream failure.
            Err(e) => {
                let message = e.to_string();
                error!("chain/rpc eth_getLogs: {message}");
                if message.contains("too wide") {
                    rpc_error(id, -32602, &message)
                } else {
                    rpc_error(id, -32000, "Upstream log query failed")
                }
            }
        };
    }

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": request.id.clone().unwrap_or(serde_json::json!(1)),
        "method": request.method,
        "params": request.params,
    });

    match client.post(&CONFIG.http_rpc_url).json(&body).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(value) => {
                // Only successful results are cached: an error is about this attempt, not about
                // the state of the chain at this block.
                if let (Some((address, data, block)), Some(result)) =
                    (&call_key, value.get("result").and_then(|r| r.as_str()))
                {
                    read_cache::put_call(
                        address,
                        data,
                        *block,
                        result.to_string(),
                        issued_at_block,
                    )
                    .await;
                }

                if request.method == "eth_blockNumber" {
                    if let Some(hex) = value.get("result").and_then(|r| r.as_str()) {
                        if let Ok(number) = u64::from_str_radix(hex.trim_start_matches("0x"), 16) {
                            // Only the number is known here. Publishing a timestamp of 0 alongside
                            // it made `/chain/head` serve an epoch date for the whole TTL, which
                            // callers compare voting deadlines against.
                            read_cache::put_block_number(number).await;
                        }
                    }
                }

                value
            }
            Err(e) => {
                error!("chain/rpc: upstream returned invalid JSON: {e}");
                rpc_error(id, -32000, "Upstream returned invalid JSON")
            }
        },
        Err(e) => {
            error!("chain/rpc: upstream request failed: {e}");
            rpc_error(id, -32000, "Upstream RPC unavailable")
        }
    }
}

/// Answer an `eth_getLogs` filter from the log index, or `None` when it cannot be answered there.
///
/// Returns logs in the JSON-RPC wire shape so the caller cannot tell an indexed answer from a
/// forwarded one — the whole point is that a standard client keeps working either way.
///
/// `None` on any doubt: a range that starts before indexing began, or reaches past what has been
/// applied, must go upstream rather than come back quietly short.
async fn logs_from_index(
    store: &web::Data<AppData>,
    params: &serde_json::Value,
) -> Option<Vec<serde_json::Value>> {
    let filter = params.get(0)?;
    let address = filter.get("address")?.as_str()?;

    // `blockHash` is an alternative to `fromBlock`/`toBlock` that names one specific block,
    // including an orphaned one. The index is keyed by height and has no way to answer it — and
    // with both range bounds absent the bounds below would default to the indexed head, so this
    // returned the head block's logs as if they were the requested block's.
    if filter.get("blockHash").is_some_and(|v| !v.is_null()) {
        return None;
    }

    if !is_log_indexed(address) {
        return None;
    }

    let repo = store.logs();
    let indexed_from = repo.coverage(address).await.ok()??;
    let indexed_head = repo.indexed_head().await.ok()??;

    let parse_tag = |value: Option<&serde_json::Value>, fallback: u64| -> Option<u64> {
        match value.and_then(|v| v.as_str()) {
            None | Some("latest") | Some("safe") | Some("finalized") => Some(fallback),
            Some("earliest") => Some(0),
            // `pending` includes blocks that are not final, which an index built from applied
            // blocks cannot speak for.
            Some("pending") => None,
            // The `0x` prefix is REQUIRED, matching what the upstream path accepts. Stripping an
            // absent prefix and parsing as hex anyway silently rewrote the range: `"1000"`
            // (decimal, a common client bug) became block 4096, and if the shifted range happened
            // to sit inside the covered span, the index answered for blocks the caller never
            // asked about — as an ordinary result array, with nothing to detect it by.
            Some(tag) if tag.starts_with("0x") => {
                u64::from_str_radix(tag.trim_start_matches("0x"), 16).ok()
            }
            Some(_) => None,
        }
    };

    // `eth_getLogs` defaults BOTH bounds to `latest`, not to genesis: a filter with no
    // `fromBlock` is a single-block query. Defaulting it to 0 turned that into a full-history
    // scan, which is both wrong and expensive.
    let from = parse_tag(filter.get("fromBlock"), indexed_head)?;
    let to = parse_tag(filter.get("toBlock"), indexed_head)?;

    // Not clamped: a request reaching past what has been applied must go upstream, because the
    // index would answer it short and look authoritative doing so.
    if from < indexed_from || to > indexed_head {
        return None;
    }

    // Only positional topic filters are served here; `eth_getLogs` also allows an array in a
    // position to mean "any of these", which the index does not implement. Anything else goes
    // upstream rather than being answered approximately.
    let topics: Vec<Option<String>> = match filter.get("topics") {
        None => Vec::new(),
        Some(serde_json::Value::Array(entries)) => {
            let mut parsed = Vec::with_capacity(entries.len());
            for entry in entries {
                match entry {
                    serde_json::Value::Null => parsed.push(None),
                    serde_json::Value::String(topic) => parsed.push(Some(topic.clone())),
                    _ => return None,
                }
            }
            parsed
        }
        Some(_) => return None,
    };

    let found = repo.query(address, from, to, &topics).await.ok()?;

    // Every field of the mined-log shape, or nothing. `blockHash` and `transactionIndex` were not
    // stored until recently, so an entry written by an older build cannot be rendered completely —
    // and a JSON-RPC client is entitled to reject a mined log that omits them. Falling through to
    // the provider is the honest answer for those; once the range is re-indexed it is served here
    // again. Emitting the fields as `null` would be the shape of a PENDING log, which is worse
    // than being slow.
    let mut entries = Vec::with_capacity(found.len());
    for log in found {
        let (Some(block_hash), Some(transaction_index)) = (log.block_hash, log.transaction_index)
        else {
            return None;
        };

        entries.push(serde_json::json!({
            "address": log.address,
            "topics": log.topics,
            "data": log.data,
            "blockNumber": format!("0x{:x}", log.block_number),
            "blockHash": block_hash,
            "transactionHash": log.transaction_hash,
            "transactionIndex": format!("0x{transaction_index:x}"),
            "logIndex": format!("0x{:x}", log.log_index),
            "removed": false,
        }));
    }

    Some(entries)
}

/// Run an `eth_getLogs` request as a series of bounded windows and concatenate the results.
async fn forward_windowed_logs(
    client: &reqwest::Client,
    params: &serde_json::Value,
) -> eyre::Result<Vec<serde_json::Value>> {
    let filter = params.get(0).cloned().unwrap_or(serde_json::json!({}));

    // A `blockHash` filter names one block and is mutually exclusive with a range. Rewriting
    // `fromBlock`/`toBlock` into it below would produce a request the node rejects, so it is
    // forwarded as-is in a single call instead.
    if filter.get("blockHash").is_some_and(|v| !v.is_null()) {
        return forward_logs_verbatim(client, &filter).await;
    }

    let head = upstream().await?.get_block_number().await?;

    // Hex block tags, `earliest`/`latest`, or absent — all normalised to numbers so the window
    // arithmetic below has something to count with. A tag that parses as none of these is an
    // error rather than a silent fallback: `"fromBlock": "1000"` (decimal, a common client bug)
    // used to resolve to the head and come back as an empty single-block scan, presented as a
    // complete answer for the range the caller actually asked for.
    let resolve = |value: Option<&serde_json::Value>, fallback: u64| -> eyre::Result<u64> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(fallback),
            Some(serde_json::Value::String(tag)) => match tag.as_str() {
                "latest" | "pending" | "safe" | "finalized" => Ok(fallback),
                "earliest" => Ok(0),
                hex if hex.starts_with("0x") => {
                    u64::from_str_radix(hex.trim_start_matches("0x"), 16)
                        .map_err(|_| eyre::eyre!("invalid block tag: {tag}"))
                }
                other => Err(eyre::eyre!("invalid block tag: {other}")),
            },
            Some(other) => Err(eyre::eyre!("invalid block tag: {other}")),
        }
    };

    // Both bounds default to `latest`, per the JSON-RPC spec.
    let from = resolve(filter.get("fromBlock"), head)?;
    let to = resolve(filter.get("toBlock"), head)?.min(head);

    if windows_for(from, to) > MAX_LOG_WINDOWS {
        return Err(eyre::eyre!(
            "range {from}-{to} is too wide; at most {} blocks per request",
            MAX_LOG_WINDOWS * LOG_WINDOW
        ));
    }

    let mut all = Vec::new();
    let mut start = from;

    while start <= to {
        let end = start.saturating_add(LOG_WINDOW - 1).min(to);

        let mut windowed = filter.clone();
        windowed["fromBlock"] = serde_json::json!(format!("0x{start:x}"));
        windowed["toBlock"] = serde_json::json!(format!("0x{end:x}"));

        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "eth_getLogs", "params": [windowed],
        });

        let response: serde_json::Value = client
            .post(&CONFIG.http_rpc_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if let Some(err) = response.get("error") {
            return Err(eyre::eyre!("upstream error: {err}"));
        }

        if let Some(serde_json::Value::Array(logs)) = response.get("result") {
            all.extend(logs.clone());
        }

        start = end.saturating_add(1);
    }

    Ok(all)
}

/// Forward one `eth_getLogs` filter unchanged, for the shapes windowing cannot express.
async fn forward_logs_verbatim(
    client: &reqwest::Client,
    filter: &serde_json::Value,
) -> eyre::Result<Vec<serde_json::Value>> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "eth_getLogs", "params": [filter],
    });

    let response: serde_json::Value = client
        .post(&CONFIG.http_rpc_url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    if let Some(err) = response.get("error") {
        return Err(eyre::eyre!("upstream error: {err}"));
    }

    match response.get("result") {
        Some(serde_json::Value::Array(logs)) => Ok(logs.clone()),
        _ => Ok(Vec::new()),
    }
}

/// Contracts these routes will serve: `INDEX_CONTRACTS` plus the ones this server is itself
/// configured against.
///
/// An empty allowlist denies everything rather than allowing everything: a misconfigured
/// deployment should fail closed, not silently become an open RPC endpoint.
///
/// The server's own contracts are implicit because refusing them is never the intended
/// configuration — the SDK's `getOnChainRoundData` reads the E3 program this server was deployed
/// to serve, and requiring the operator to name it a second time in `INDEX_CONTRACTS` turned a
/// forgotten variable into "the SDK cannot read the round it just told you about".
fn is_allowed(address: &Address) -> bool {
    let configured = [
        CONFIG.e3_program_address.as_str(),
        CONFIG.interfold_address.as_str(),
        CONFIG.ciphernode_registry_address.as_str(),
        CONFIG.fee_token_address.as_str(),
        CONFIG.crisp_voting_token.as_deref().unwrap_or(""),
    ];

    let listed = CONFIG
        .index_contracts
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim);

    configured
        .into_iter()
        .chain(listed)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| Address::from_str(entry).ok())
        .any(|allowed| allowed == *address)
}

fn parse_address(value: &str) -> Option<Address> {
    Address::from_str(value.trim()).ok()
}

/// Whether an address's logs are being indexed RIGHT NOW, per the live configuration.
///
/// Checked in addition to the stored coverage record, because a coverage record outlives the
/// configuration that created it: the store has no delete, so removing a contract from
/// `INDEX_LOG_CONTRACTS` — the documented way to stop paying for a chatty contract's logs — left
/// its record behind while the cursor kept advancing. Every query then passed the coverage test
/// and was answered from a frozen index, missing every event since the removal, and by design
/// indistinguishable from an upstream answer.
fn is_log_indexed(address: &str) -> bool {
    let Some(address) = parse_address(address) else {
        return false;
    };

    CONFIG
        .index_log_contracts
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| Address::from_str(entry).ok())
        .any(|indexed| indexed == address)
}

#[derive(Debug, Serialize)]
pub struct HeadResponse {
    pub block_number: u64,
    pub timestamp: u64,
    pub chain_id: u64,
}

/// The chain head: block number and its timestamp.
///
/// Replaces a `useBlockNumber({ watch: true })` poll per hook with one call, and returns the
/// timestamp alongside so callers deciding whether a voting window has closed do not need a
/// second round trip for the block.
async fn head() -> impl Responder {
    // The most-polled call in the app by an order of magnitude: several hooks per client watch it
    // on a timer. Serving a few seconds old head collapses that crowd into one upstream request.
    // Only a fully-known head is served: an entry learned from `eth_blockNumber` carries no
    // timestamp, and callers compare voting deadlines against this field.
    if let Some(cached) = read_cache::head().await {
        if let Some(timestamp) = cached.timestamp {
            read_cache::record_head(true).await;
            return HttpResponse::Ok().json(HeadResponse {
                block_number: cached.block_number,
                timestamp,
                chain_id: CONFIG.chain_id,
            });
        }
    }
    read_cache::record_head(false).await;

    let provider = match upstream().await {
        Ok(p) => p,
        Err(e) => {
            error!("chain/head: provider unavailable: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    match provider.get_block_by_number(BlockNumberOrTag::Latest).await {
        Ok(Some(block)) => {
            read_cache::put_head(block.header.number, block.header.timestamp).await;
            HttpResponse::Ok().json(HeadResponse {
                block_number: block.header.number,
                timestamp: block.header.timestamp,
                chain_id: CONFIG.chain_id,
            })
        }
        Ok(None) => HttpResponse::ServiceUnavailable().json(JsonResponse {
            response: "Upstream RPC returned no head block".to_string(),
        }),
        Err(e) => {
            error!("chain/head: {e}");
            HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            })
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ReadCall {
    pub address: String,
    /// ABI-encoded calldata, hex with or without the `0x` prefix. The caller owns the encoding;
    /// this route deliberately knows nothing about ABIs so it does not have to be redeployed
    /// every time a client wants a different view function.
    pub data: String,
    /// Optional historical block. Omit for latest.
    #[serde(default)]
    pub block_number: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ReadRequest {
    pub calls: Vec<ReadCall>,
}

#[derive(Debug, Serialize)]
pub struct ReadResult {
    /// Hex-encoded return data, or null when the call reverted.
    pub result: Option<String>,
    /// Revert reason when the call failed, so a caller can tell "reverted" from "returned empty".
    pub error: Option<String>,
}

/// `eth_call` against allowlisted contracts, batched.
///
/// Point reads (a balance, a delegation, a proposal struct) are answered from the chain rather
/// than from the index on purpose: they are per-user and change constantly, so an indexed copy
/// would have to mirror every transfer and delegation to stay correct, and a stale answer here is
/// not a slow UI — it is the wrong balance or a voter wrongly told they are ineligible.
async fn read(request: web::Json<ReadRequest>) -> impl Responder {
    if request.calls.is_empty() {
        return HttpResponse::Ok().json(Vec::<ReadResult>::new());
    }

    if request.calls.len() > MAX_BATCH {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: format!("At most {MAX_BATCH} calls per request"),
        });
    }

    let provider = match upstream().await {
        Ok(p) => p,
        Err(e) => {
            error!("chain/read: provider unavailable: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    let mut results = Vec::with_capacity(request.calls.len());
    let issued_at_block = read_cache::current_latest_block().await;

    for call in &request.calls {
        let Some(address) = parse_address(&call.address) else {
            return HttpResponse::BadRequest().json(JsonResponse {
                response: format!("Invalid address: {}", call.address),
            });
        };

        if !is_allowed(&address) {
            return HttpResponse::Forbidden().json(JsonResponse {
                response: format!("Address not served by this indexer: {address}"),
            });
        }

        let Ok(data) = Bytes::from_str(call.data.trim()) else {
            return HttpResponse::BadRequest().json(JsonResponse {
                response: "Invalid calldata".to_string(),
            });
        };

        // Within one block an `eth_call` at `latest` is deterministic, so a repeat is a redundant
        // question rather than a fresher answer — whoever asked first already paid for it.
        if let Some(hit) = read_cache::call(&call.address, &call.data, call.block_number).await {
            read_cache::record_call(true).await;
            results.push(ReadResult {
                result: Some(hit),
                error: None,
            });
            continue;
        }
        read_cache::record_call(false).await;

        let tx = TransactionRequest::default()
            .with_to(address)
            .with_input(data);

        let pending = match call.block_number {
            Some(number) => provider
                .call(tx)
                .block(BlockNumberOrTag::Number(number).into()),
            None => provider.call(tx),
        };

        // A revert is reported per call rather than failing the batch: callers routinely probe
        // functions a contract may not implement (the IVotes and proxy probes both do), and one
        // expected revert must not discard the other results in the batch.
        match pending.await {
            Ok(output) => {
                let encoded = output.to_string();
                read_cache::put_call(
                    &call.address,
                    &call.data,
                    call.block_number,
                    encoded.clone(),
                    issued_at_block,
                )
                .await;
                results.push(ReadResult {
                    result: Some(encoded),
                    error: None,
                })
            }
            Err(e) => results.push(ReadResult {
                result: None,
                error: Some(e.to_string()),
            }),
        }
    }

    HttpResponse::Ok().json(results)
}

#[derive(Debug, Deserialize)]
pub struct LogsRequest {
    pub address: String,
    /// Topic filters, positional. `null` in any position matches anything, mirroring `eth_getLogs`.
    #[serde(default)]
    pub topics: Vec<Option<String>>,
    #[serde(default)]
    pub from_block: Option<u64>,
    #[serde(default)]
    pub to_block: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: Option<u64>,
    pub transaction_hash: Option<String>,
    pub log_index: Option<u64>,
}

/// `eth_getLogs` over an arbitrary range, windowed server-side.
///
/// The window is the point: a caller asking for a contract's whole history gets it in one
/// request, instead of reimplementing range-splitting against whatever cap the provider enforces.
async fn logs(request: web::Json<LogsRequest>, store: web::Data<AppData>) -> impl Responder {
    let Some(address) = parse_address(&request.address) else {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: format!("Invalid address: {}", request.address),
        });
    };

    if !is_allowed(&address) {
        return HttpResponse::Forbidden().json(JsonResponse {
            response: format!("Address not served by this indexer: {address}"),
        });
    }

    // Served from the index when it demonstrably covers the range: the indexer watches these
    // contracts anyway, so the logs are already here and answering locally turns a scan of a
    // contract's whole history into one read. Coverage is checked at both ends — a range starting
    // before indexing began, or reaching past what has been applied, would come back short, and a
    // silently short answer is worse than a slower correct one.
    let repo = store.logs();
    if is_log_indexed(&request.address) {
        if let (Ok(Some(indexed_from)), Ok(Some(indexed_head))) = (
            repo.coverage(&request.address).await,
            repo.indexed_head().await,
        ) {
            let from = request.from_block.unwrap_or(0);
            // Compared BEFORE clamping. Clamping first made this test tautological, so a caller
            // asking past the indexed head got a quietly truncated 200 instead of the upstream
            // answer.
            let requested_to = request.to_block.unwrap_or(indexed_head);

            if from >= indexed_from && requested_to <= indexed_head {
                let to = requested_to;
                let topics: Vec<Option<String>> = request.topics.clone();
                match repo.query(&request.address, from, to, &topics).await {
                    Ok(found) => {
                        read_cache::record_logs(true).await;
                        return HttpResponse::Ok().json(
                            found
                                .into_iter()
                                .map(|log| LogEntry {
                                    address: log.address,
                                    topics: log.topics,
                                    data: log.data,
                                    block_number: Some(log.block_number),
                                    transaction_hash: log.transaction_hash,
                                    log_index: Some(log.log_index),
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                    Err(e) => error!("chain/logs: index read failed, falling back upstream: {e}"),
                }
            }
        }
    }

    // Reached only when the index could not answer, so the counter reflects real fallthrough.
    read_cache::record_logs(false).await;

    let provider = match upstream().await {
        Ok(p) => p,
        Err(e) => {
            error!("chain/logs: provider unavailable: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    let head = match provider.get_block_number().await {
        Ok(n) => n,
        Err(e) => {
            error!("chain/logs: head lookup failed: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    let from = request.from_block.unwrap_or(0);
    let to = request.to_block.unwrap_or(head).min(head);

    if from > to {
        return HttpResponse::Ok().json(Vec::<LogEntry>::new());
    }

    if windows_for(from, to) > MAX_LOG_WINDOWS {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: format!(
                "Range {from}-{to} is too wide; at most {} blocks per request. Start from the \
                 contract's deployment block.",
                MAX_LOG_WINDOWS * LOG_WINDOW
            ),
        });
    }

    // Refused, not truncated. A log has at most four topics, so a fifth is a malformed filter —
    // and `.take(4)` answered it by quietly dropping the extras and returning logs that do not
    // match what was asked for, as a normal 200. The index path passes the whole vector to
    // `repo.query`, so the two paths also disagreed on the same request.
    if request.topics.len() > 4 {
        return HttpResponse::BadRequest().json(JsonResponse {
            response: "At most 4 topic positions may be filtered".to_string(),
        });
    }

    let mut base = Filter::new().address(address);
    for (position, topic) in request.topics.iter().enumerate() {
        let Some(topic) = topic else { continue };
        let Ok(hash) = B256::from_str(topic.trim()) else {
            return HttpResponse::BadRequest().json(JsonResponse {
                response: format!("Invalid topic at position {position}"),
            });
        };
        base = match position {
            0 => base.event_signature(hash),
            1 => base.topic1(hash),
            2 => base.topic2(hash),
            _ => base.topic3(hash),
        };
    }

    let mut entries: Vec<LogEntry> = Vec::new();
    let mut start = from;

    while start <= to {
        let end = start.saturating_add(LOG_WINDOW - 1).min(to);

        let filter = base
            .clone()
            .from_block(BlockNumberOrTag::Number(start))
            .to_block(BlockNumberOrTag::Number(end));

        match provider.get_logs(&filter).await {
            Ok(found) => entries.extend(found.into_iter().map(|log| LogEntry {
                address: log.address().to_string(),
                topics: log.topics().iter().map(|t| t.to_string()).collect(),
                data: log.data().data.to_string(),
                block_number: log.block_number,
                transaction_hash: log.transaction_hash.map(|h| h.to_string()),
                log_index: log.log_index,
            })),
            Err(e) => {
                error!("chain/logs: window {start}-{end} failed: {e}");
                return HttpResponse::ServiceUnavailable().json(JsonResponse {
                    response: "Upstream RPC rejected a log query".to_string(),
                });
            }
        }

        start = end.saturating_add(1);
    }

    entries.sort_by_key(|entry| (entry.block_number, entry.log_index));

    HttpResponse::Ok().json(entries)
}

#[derive(Debug, Deserialize)]
pub struct BlockAtTimestampRequest {
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct BlockAtTimestampResponse {
    pub block_number: u64,
    pub timestamp: u64,
}

/// The last block at or before a timestamp.
///
/// Clients need this to turn a proposal's snapshot timepoint into a block, and the obvious
/// client-side implementation is a binary search that costs `O(log n)` `eth_getBlockByNumber`
/// calls per lookup. Doing it here spends those on one connection instead of the browser's.
async fn block_at_timestamp(request: web::Json<BlockAtTimestampRequest>) -> impl Responder {
    let provider = match upstream().await {
        Ok(p) => p,
        Err(e) => {
            error!("chain/block-at-timestamp: provider unavailable: {e}");
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            });
        }
    };

    let target = request.timestamp;

    let head_block = match provider.get_block_by_number(BlockNumberOrTag::Latest).await {
        Ok(Some(b)) => b,
        _ => {
            return HttpResponse::ServiceUnavailable().json(JsonResponse {
                response: "Upstream RPC unavailable".to_string(),
            })
        }
    };

    // A timestamp in the future resolves to the head rather than erroring: callers ask about
    // windows that have not closed yet, and "the latest block we have" is the honest answer.
    if head_block.header.timestamp <= target {
        return HttpResponse::Ok().json(BlockAtTimestampResponse {
            block_number: head_block.header.number,
            timestamp: head_block.header.timestamp,
        });
    }

    let mut low = 0u64;
    let mut high = head_block.header.number;
    let mut best: Option<(u64, u64)> = None;

    while low <= high {
        let mid = low + (high - low) / 2;

        // A failed probe must abort the search, not end it: the bisection has only ruled out half
        // the range at each step, so whatever `best` holds is a partial answer. Returning it was
        // a 200 carrying block 0 — and a caller resolving a snapshot timepoint would then read
        // `getPastVotes(voter, 0)` and report every voter ineligible.
        let block = match provider
            .get_block_by_number(BlockNumberOrTag::Number(mid))
            .await
        {
            Ok(Some(b)) => b,
            Ok(None) => {
                error!("chain/block-at-timestamp: block {mid} missing during bisection");
                return HttpResponse::ServiceUnavailable().json(JsonResponse {
                    response: "Upstream RPC could not resolve the timestamp".to_string(),
                });
            }
            Err(e) => {
                error!("chain/block-at-timestamp: block {mid} lookup failed: {e}");
                return HttpResponse::ServiceUnavailable().json(JsonResponse {
                    response: "Upstream RPC unavailable".to_string(),
                });
            }
        };

        if block.header.timestamp <= target {
            best = Some((block.header.number, block.header.timestamp));
            low = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            high = mid - 1;
        }
    }

    // `best` is empty only when even genesis is later than the target — the timestamp predates the
    // chain, so no block satisfies the request. Answering `block_number: 0, timestamp: 0` invented
    // a timestamp genesis does not have, and a caller comparing it against a voting window read an
    // epoch date. There is no honest number here, so say so.
    let Some((block_number, timestamp)) = best else {
        return HttpResponse::NotFound().json(JsonResponse {
            response: "No block exists at or before that timestamp".to_string(),
        });
    };

    HttpResponse::Ok().json(BlockAtTimestampResponse {
        block_number,
        timestamp,
    })
}
