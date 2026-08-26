// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! A generic log index for the contracts on the `/chain/*` allowlist.
//!
//! Deliberately untyped. The alternative — a handler per event, decoding into a domain model —
//! would mean this server knowing what a proposal or a delegation is, and being redeployed
//! whenever a client wants a different event. Storing logs as they come keeps the meaning on the
//! client's side, and still turns "scan a contract's whole history" from a few hundred upstream
//! requests into one local read.
//!
//! Logs are bucketed by block rather than written one key per log, because the store's interface
//! is get/insert over whole values: a key per log makes range queries impossible without a scan,
//! and a single key per address makes every append rewrite the entire history. A bucket bounds
//! both — a range query touches `range / BUCKET_SIZE` keys, and an append rewrites one bucket.

use e3_sdk::indexer::{DataStore, SharedStore, INDEXER_CURSOR_KEY};
use eyre::Result;
use serde::{Deserialize, Serialize};

/// Blocks per bucket. At ~12s blocks this is roughly a day and a half of history per key, so a
/// query spanning a typical deployment's lifetime reads tens of keys, not thousands.
const BUCKET_SIZE: u64 = 10_000;

/// One indexed log, in the shape the `/chain/logs` route returns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredLog {
    /// True when the node reported this log as orphaned by a reorg. Stored entries are never
    /// `removed` — the flag only travels far enough to delete what it supersedes.
    #[serde(default)]
    pub removed: bool,
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: Option<String>,
    pub log_index: u64,
    /// Both `#[serde(default)]` so entries written before these were stored still deserialise.
    ///
    /// They are part of the mined-log shape `eth_getLogs` returns, and the point of the index is
    /// that a client cannot tell an indexed answer from a forwarded one. An entry missing either
    /// is served upstream instead — see `logs_from_index`.
    #[serde(default)]
    pub block_hash: Option<String>,
    #[serde(default)]
    pub transaction_index: Option<u64>,
}

/// The range of blocks indexed for one address.
///
/// `from_block` is where indexing began, and is what makes a coverage question answerable: a
/// query starting earlier than this cannot be served from the store, however much is in it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogCoverage {
    pub from_block: u64,
}

pub struct LogRepository<S: DataStore> {
    store: SharedStore<S>,
}

impl<S: DataStore> LogRepository<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self { store }
    }

    /// Addresses are lowercased in keys so a checksummed request and an indexed log agree.
    fn bucket_key(address: &str, bucket: u64) -> String {
        format!("_logs:{}:{}", address.to_lowercase(), bucket)
    }

    fn coverage_key(address: &str) -> String {
        format!("_logs:{}:coverage", address.to_lowercase())
    }

    /// Record a log, replacing any entry already stored at the same position.
    ///
    /// Idempotent on `(block_number, log_index)` because the same log legitimately arrives twice:
    /// the indexer catches up to the head and then subscribes, and the overlap between the two is
    /// deliberate — a gap would be worse than a duplicate.
    ///
    /// REPLACES rather than skips, because that position is exactly where a reorg puts the
    /// canonical log that supersedes an orphaned one. Skipping the second arrival would pin the
    /// orphan permanently.
    pub async fn append(&mut self, log: StoredLog) -> Result<()> {
        let bucket = log.block_number / BUCKET_SIZE;
        let key = Self::bucket_key(&log.address, bucket);

        let mut entries: Vec<StoredLog> = self
            .store
            .get(&key)
            .await
            .map_err(|e| eyre::eyre!("reading log bucket {key} failed: {e}"))?
            .unwrap_or_default();

        let position = entries
            .iter()
            .position(|e| e.block_number == log.block_number && e.log_index == log.log_index);

        // A node reports an orphaned log with `removed: true` when a reorg drops it. Retaining it
        // would leave the index asserting an event that no longer happened.
        if log.removed {
            match position {
                Some(index) => {
                    entries.remove(index);
                }
                None => return Ok(()),
            }
        } else if let Some(index) = position {
            entries[index] = log.clone();
        } else {
            // Appended unsorted: `query` orders what it returns, so sorting the whole bucket on
            // every insert would pay O(n log n) per log for an order only the reader cares about.
            entries.push(log.clone());
        }

        self.store
            .insert(&key, &entries)
            .await
            .map_err(|e| eyre::eyre!("writing log bucket {key} failed: {e}"))?;

        self.widen_coverage(&log.address, log.block_number).await
    }

    /// Extend the recorded coverage downwards as older blocks are indexed.
    async fn widen_coverage(&mut self, address: &str, block_number: u64) -> Result<()> {
        let key = Self::coverage_key(address);

        let current: Option<LogCoverage> = self
            .store
            .get(&key)
            .await
            .map_err(|e| eyre::eyre!("reading log coverage {key} failed: {e}"))?;

        let from_block = match current {
            Some(existing) if existing.from_block <= block_number => return Ok(()),
            Some(existing) => existing.from_block.min(block_number),
            None => block_number,
        };

        self.store
            .insert(&key, &LogCoverage { from_block })
            .await
            .map_err(|e| eyre::eyre!("writing log coverage {key} failed: {e}"))?;

        Ok(())
    }

    /// Declare that indexing for an address began at `from_block`, even before any log arrives.
    ///
    /// Without this a contract that has emitted nothing yet looks uncovered forever, and every
    /// query for it would fall through to the upstream provider despite the index being complete.
    pub async fn ensure_coverage_from(&mut self, address: &str, from_block: u64) -> Result<()> {
        let key = Self::coverage_key(address);

        let current: Option<LogCoverage> = self
            .store
            .get(&key)
            .await
            .map_err(|e| eyre::eyre!("reading log coverage {key} failed: {e}"))?;

        if current.is_some() {
            return Ok(());
        }

        self.store
            .insert(&key, &LogCoverage { from_block })
            .await
            .map_err(|e| eyre::eyre!("writing log coverage {key} failed: {e}"))?;

        Ok(())
    }

    /// Re-base an address's coverage to `from_block`, discarding an earlier claim.
    ///
    /// Needed because coverage outlives the configuration that produced it. An address dropped
    /// from `INDEX_LOG_CONTRACTS` and later restored still carries the record from its first run,
    /// which claims history reaching back before the gap when nothing was indexed. Reads are
    /// gated on the live configuration, so the stale record cannot be served while the address is
    /// absent — but when it returns, the claim has to be narrowed to what will actually be there.
    ///
    /// Only ever narrows: a record already starting later is left alone.
    pub async fn rebase_coverage(&mut self, address: &str, from_block: u64) -> Result<()> {
        let key = Self::coverage_key(address);

        let current: Option<LogCoverage> = self
            .store
            .get(&key)
            .await
            .map_err(|e| eyre::eyre!("reading log coverage {key} failed: {e}"))?;

        if current.is_none_or(|existing| existing.from_block >= from_block) {
            return Ok(());
        }

        self.store
            .insert(&key, &LogCoverage { from_block })
            .await
            .map_err(|e| eyre::eyre!("writing log coverage {key} failed: {e}"))?;

        Ok(())
    }

    /// The highest block the indexer has fully applied.
    ///
    /// The upper bound of what the store can answer: a query reaching past it would silently
    /// report "no logs" for blocks that simply have not been read yet, which is worse than
    /// forwarding the question upstream.
    pub async fn indexed_head(&self) -> Result<Option<u64>> {
        self.store
            .get(INDEXER_CURSOR_KEY)
            .await
            .map_err(|e| eyre::eyre!("reading the indexer cursor failed: {e}"))
    }

    /// The first block indexed for an address, if any.
    pub async fn coverage(&self, address: &str) -> Result<Option<u64>> {
        let key = Self::coverage_key(address);
        let coverage: Option<LogCoverage> = self
            .store
            .get(&key)
            .await
            .map_err(|e| eyre::eyre!("reading log coverage {key} failed: {e}"))?;

        Ok(coverage.map(|c| c.from_block))
    }

    /// Logs for an address in `[from, to]`, optionally filtered by positional topics.
    ///
    /// `None` in a topic position matches anything, mirroring `eth_getLogs` so a caller can pass
    /// the same filter to either source and get the same answer.
    pub async fn query(
        &self,
        address: &str,
        from: u64,
        to: u64,
        topics: &[Option<String>],
    ) -> Result<Vec<StoredLog>> {
        if from > to {
            return Ok(Vec::new());
        }

        let mut found = Vec::new();

        for bucket in (from / BUCKET_SIZE)..=(to / BUCKET_SIZE) {
            let key = Self::bucket_key(address, bucket);

            let entries: Vec<StoredLog> = self
                .store
                .get(&key)
                .await
                .map_err(|e| eyre::eyre!("reading log bucket {key} failed: {e}"))?
                .unwrap_or_default();

            for entry in entries {
                if entry.block_number < from || entry.block_number > to {
                    continue;
                }

                let matches = topics.iter().enumerate().all(|(position, wanted)| {
                    let Some(wanted) = wanted else { return true };
                    entry
                        .topics
                        .get(position)
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(wanted))
                });

                if matches {
                    found.push(entry);
                }
            }
        }

        found.sort_by_key(|entry| (entry.block_number, entry.log_index));
        Ok(found)
    }
}
