// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Deduplication of chain reads across clients.
//!
//! The log index already answers historical queries without touching the provider, but point
//! reads did not: every `eth_call` and every `eth_blockNumber` poll was forwarded, so N clients
//! asking the same question cost N upstream requests. With a handful of hooks polling the head
//! on a timer, that is the highest-volume traffic the server sees.
//!
//! The correctness argument for caching `eth_call` is not "close enough for a while" — it is
//! exact. Contract state can only change at a block boundary, so two `eth_call`s at `latest`
//! within the same block MUST return the same bytes; the second one is a redundant question, not
//! a fresher answer. The cache is therefore keyed by block and dropped wholesale when the head
//! moves, which also bounds its memory to one block's worth of distinct reads.
//!
//! A call pinned to an explicit historical block is immutable forever, so those are kept across
//! block changes and evicted only by the size cap.
//!
//! Deliberately in memory rather than in sled: this is high-churn, worthless after a restart, and
//! writing it to the same store the indexer holds a global lock on would slow down the thing it is
//! meant to speed up.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// How long a cached head is served before the provider is asked again.
///
/// Sized well under a block time: clients poll far faster than blocks are produced, so this
/// collapses a crowd of pollers into at most one upstream call every few seconds while never
/// serving a head that could be more than one block stale.
const HEAD_TTL: Duration = Duration::from_secs(3);

/// Hard cap on cached calls, so a client enumerating distinct calldata cannot grow this without
/// bound between blocks.
const MAX_ENTRIES: usize = 20_000;

/// Backstop lifetime for the `latest` call cache.
///
/// The block-boundary invalidation below is the correct rule, but it only fires when something
/// reads the head — so if head refreshes stop (a client that only calls `/chain/read`, or an
/// upstream that starts failing `/chain/head`), entries from an old block would be served as
/// `latest` indefinitely. This bounds that to a span shorter than a block time, so the worst case
/// degrades to "one block stale" rather than "stale until the process restarts".
const LATEST_CALL_TTL: Duration = Duration::from_secs(6);

#[derive(Clone, Copy)]
pub struct CachedHead {
    pub block_number: u64,
    /// `None` when the head was learned from `eth_blockNumber`, which carries no timestamp.
    /// Callers that need the time must fall through rather than be handed a zero.
    pub timestamp: Option<u64>,
}

#[derive(Default)]
struct Inner {
    head: Option<(CachedHead, Instant)>,
    /// Reads at `latest`, valid only for `latest_block`.
    latest_calls: HashMap<(String, String), String>,
    latest_block: u64,
    /// When `latest_block` was last advanced, for the TTL backstop.
    latest_block_at: Option<Instant>,
    /// Reads pinned to a historical block, keyed by `(address, calldata, block)`.
    historical_calls: HashMap<(String, String, u64), String>,
}

static CACHE: Lazy<RwLock<Inner>> = Lazy::new(|| RwLock::new(Inner::default()));

/// The cached head, if it is still fresh.
pub async fn head() -> Option<CachedHead> {
    let guard = CACHE.read().await;
    let (head, fetched_at) = guard.head?;

    (fetched_at.elapsed() < HEAD_TTL).then_some(head)
}

/// Record a freshly read head.
///
/// Advancing the head invalidates every `latest` call in one step, which is both the correct
/// moment to do it and the reason this cache cannot leak: the map lives for exactly one block.
pub async fn put_head(block_number: u64, timestamp: u64) {
    store_head(block_number, Some(timestamp)).await
}

/// Record a head learned without its timestamp (the `eth_blockNumber` path).
pub async fn put_block_number(block_number: u64) {
    store_head(block_number, None).await
}

async fn store_head(block_number: u64, timestamp: Option<u64>) {
    let mut guard = CACHE.write().await;

    // `!=` rather than `>`: a reorg that lowers the head still means the state these results
    // describe is gone, and a `>` test would keep serving the orphaned block's answers.
    if block_number != guard.latest_block {
        guard.latest_block = block_number;
        guard.latest_block_at = Some(Instant::now());
        guard.latest_calls.clear();
    }

    // A timestamped entry is strictly more useful than a bare number for the same block, so a
    // number-only update must not overwrite one.
    if timestamp.is_none() {
        if let Some((existing, _)) = guard.head {
            if existing.block_number == block_number && existing.timestamp.is_some() {
                return;
            }
        }
    }

    guard.head = Some((
        CachedHead {
            block_number,
            timestamp,
        },
        Instant::now(),
    ));
}

/// The block number of the cached head, whether or not its timestamp is known.
pub async fn head_number() -> Option<u64> {
    let guard = CACHE.read().await;
    let (head, fetched_at) = guard.head?;

    (fetched_at.elapsed() < HEAD_TTL).then_some(head.block_number)
}

/// The block the `latest` call cache is currently keyed to, for compare-and-set on insert.
pub async fn current_latest_block() -> u64 {
    CACHE.read().await.latest_block
}

fn normalise(address: &str, data: &str) -> (String, String) {
    (address.to_lowercase(), data.to_lowercase())
}

/// A cached `eth_call` result, if one is valid for the block being asked about.
pub async fn call(address: &str, data: &str, block: Option<u64>) -> Option<String> {
    let (address, data) = normalise(address, data);
    let guard = CACHE.read().await;

    match block {
        Some(block) => guard.historical_calls.get(&(address, data, block)).cloned(),
        None => {
            // Never serve a `latest` result without knowing which block it belongs to: an entry
            // from a previous head would be silently stale.
            if guard.latest_block == 0 {
                return None;
            }
            // Nothing has confirmed the head recently enough for these to still describe it.
            if guard
                .latest_block_at
                .is_none_or(|at| at.elapsed() >= LATEST_CALL_TTL)
            {
                return None;
            }
            guard.latest_calls.get(&(address, data)).cloned()
        }
    }
}

/// Store an `eth_call` result.
pub async fn put_call(
    address: &str,
    data: &str,
    block: Option<u64>,
    result: String,
    // The block the `latest` cache was keyed to when the call was ISSUED. If the head has moved
    // since, this result describes the previous block and filing it under the new one would serve
    // stale bytes as current.
    observed_at_block: u64,
) {
    let (address, data) = normalise(address, data);
    let mut guard = CACHE.write().await;

    if block.is_none() && guard.latest_block != observed_at_block {
        return;
    }

    match block {
        Some(block) => {
            if guard.historical_calls.len() >= MAX_ENTRIES {
                guard.historical_calls.clear();
            }
            guard
                .historical_calls
                .insert((address, data, block), result);
        }
        None => {
            if guard.latest_block == 0 {
                return;
            }
            if guard.latest_calls.len() >= MAX_ENTRIES {
                guard.latest_calls.clear();
            }
            guard.latest_calls.insert((address, data), result);
        }
    }
}

/// Counters for the `/chain/stats` route, so the saving is observable rather than asserted.
#[derive(Default)]
pub struct Counters {
    pub call_hits: u64,
    pub call_misses: u64,
    pub head_hits: u64,
    pub head_misses: u64,
    pub log_index_hits: u64,
    pub log_upstream: u64,
}

pub static COUNTERS: Lazy<RwLock<Counters>> = Lazy::new(|| RwLock::new(Counters::default()));

pub async fn record_call(hit: bool) {
    let mut c = COUNTERS.write().await;
    if hit {
        c.call_hits += 1
    } else {
        c.call_misses += 1
    }
}

pub async fn record_head(hit: bool) {
    let mut c = COUNTERS.write().await;
    if hit {
        c.head_hits += 1
    } else {
        c.head_misses += 1
    }
}

pub async fn record_logs(from_index: bool) {
    let mut c = COUNTERS.write().await;
    if from_index {
        c.log_index_hits += 1
    } else {
        c.log_upstream += 1
    }
}
