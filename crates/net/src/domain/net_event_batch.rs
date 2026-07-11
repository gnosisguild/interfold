// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::fmt::Debug;

use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use e3_events::AggregateId;
use serde::Serialize;
use tokio::time::Instant;
use tracing::info;

use crate::{
    direct_requester::{DirectRequester, WithPeer, WithoutPeer},
    domain::sync_coordinator::effective_sync_limit,
    domain::wire::{decode, MAX_DIRECT_MESSAGE_BYTES},
    events::PeerTarget,
};

/// Startup sync is a recovery aid, not an unbounded bulk-transfer protocol. These limits prevent
/// a peer from keeping a node in startup indefinitely or retaining an attacker-controlled history
/// in memory. Operators with a larger legitimate gap must use the controlled resync procedure.
const MAX_SYNC_FETCH_PAGES: usize = 512;
const MAX_SYNC_FETCH_EVENTS: usize = 50_000;
const MAX_SYNC_FETCH_BYTES: usize = 128 * 1024 * 1024;
const MAX_SYNC_FETCH_DURATION: Duration = Duration::from_secs(5 * 60);

pub(crate) struct SyncFetchBudget {
    started: Instant,
    pages: usize,
    events: usize,
    bytes: usize,
    max_pages: usize,
    max_events: usize,
    max_bytes: usize,
    max_duration: Duration,
    exhausted: bool,
}

impl SyncFetchBudget {
    pub(crate) fn production() -> Self {
        Self::new(
            MAX_SYNC_FETCH_PAGES,
            MAX_SYNC_FETCH_EVENTS,
            MAX_SYNC_FETCH_BYTES,
        )
    }

    fn new(max_pages: usize, max_events: usize, max_bytes: usize) -> Self {
        Self {
            started: Instant::now(),
            pages: 0,
            events: 0,
            bytes: 0,
            max_pages,
            max_events,
            max_bytes,
            max_duration: MAX_SYNC_FETCH_DURATION,
            exhausted: false,
        }
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    fn remaining(&mut self) -> Result<Duration> {
        match self.max_duration.checked_sub(self.started.elapsed()) {
            Some(remaining) => Ok(remaining),
            None => {
                self.exhausted = true;
                bail!("historical sync exceeded total deadline")
            }
        }
    }

    fn record_page(&mut self, event_count: usize, encoded_bytes: usize) -> Result<()> {
        let pages = self
            .pages
            .checked_add(1)
            .context("sync page count overflow")?;
        let events = self
            .events
            .checked_add(event_count)
            .context("sync event count overflow")?;
        let bytes = self
            .bytes
            .checked_add(encoded_bytes)
            .context("sync byte count overflow")?;

        if pages > self.max_pages {
            self.exhausted = true;
            bail!("historical sync exceeded page limit ({})", self.max_pages);
        }
        if events > self.max_events {
            self.exhausted = true;
            bail!("historical sync exceeded event limit ({})", self.max_events);
        }
        if bytes > self.max_bytes {
            self.exhausted = true;
            bail!("historical sync exceeded byte limit ({})", self.max_bytes);
        }

        self.pages = pages;
        self.events = events;
        self.bytes = bytes;
        Ok(())
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum BatchCursor {
    Done,
    Next(u128),
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct EventBatch<E: Debug> {
    pub events: Vec<E>,
    pub next: BatchCursor,
    pub aggregate_id: AggregateId,
}

impl<E: Debug> TryFrom<Vec<u8>> for EventBatch<E>
where
    E: serde::de::DeserializeOwned,
{
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        decode(&value, MAX_DIRECT_MESSAGE_BYTES).context("failed to deserialize EventBatch")
    }
}

impl<E: Debug> TryFrom<EventBatch<E>> for Vec<u8>
where
    E: serde::Serialize,
{
    type Error = anyhow::Error;

    fn try_from(value: EventBatch<E>) -> Result<Self> {
        bincode::serialize(&value).context("failed to serialize EventBatch")
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct FetchEventsSince {
    aggregate_id: AggregateId,
    since: u128,
    limit: usize,
}

impl FetchEventsSince {
    pub fn new(aggregate_id: AggregateId, since: u128, limit: usize) -> Self {
        Self {
            aggregate_id,
            since,
            limit,
        }
    }

    pub fn aggregate_id(&self) -> AggregateId {
        self.aggregate_id
    }

    pub fn since(&self) -> u128 {
        self.since
    }

    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl TryFrom<FetchEventsSince> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(value: FetchEventsSince) -> Result<Self> {
        bincode::serialize(&value).context("failed to serialize FetchEventsSince")
    }
}

impl TryFrom<Vec<u8>> for FetchEventsSince {
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        decode(&value, MAX_DIRECT_MESSAGE_BYTES).context("failed to deserialize FetchEventsSince")
    }
}

pub async fn fetch_events_since<E>(
    requester: &DirectRequester<WithPeer>,
    request: FetchEventsSince,
) -> Result<EventBatch<E>>
where
    E: Debug + TryFrom<Vec<u8>> + Send + Sync + 'static,
    EventBatch<E>: TryFrom<Vec<u8>>,
{
    requester.request(request).await
}

#[cfg(test)]
async fn fetch_all_batched_events<E>(
    requester: DirectRequester<WithoutPeer>,
    peer: PeerTarget,
    aggregate_id: AggregateId,
    since: u128,
    batch_size: usize,
) -> Result<Vec<E>>
where
    E: Debug + Serialize + TryFrom<Vec<u8>> + Send + Sync + 'static,
    EventBatch<E>: TryFrom<Vec<u8>>,
{
    let mut budget = SyncFetchBudget::production();
    fetch_all_batched_events_with_budget(
        requester,
        peer,
        aggregate_id,
        since,
        batch_size,
        &mut budget,
    )
    .await
}

pub(crate) async fn fetch_all_batched_events_with_budget<E>(
    requester: DirectRequester<WithoutPeer>,
    peer: PeerTarget,
    aggregate_id: AggregateId,
    since: u128,
    batch_size: usize,
    budget: &mut SyncFetchBudget,
) -> Result<Vec<E>>
where
    E: Debug + Serialize + TryFrom<Vec<u8>> + Send + Sync + 'static,
    EventBatch<E>: TryFrom<Vec<u8>>,
{
    ensure!(batch_size > 0, "sync batch size must be greater than 0");
    let batch_size = effective_sync_limit(batch_size);
    let requester = requester.to(peer);
    let mut all_events = Vec::new();
    let mut cursor = since;

    loop {
        let request = FetchEventsSince::new(aggregate_id, cursor, batch_size);
        info!(
            "Fetching batch aggregate={} cursor={} batch_size={}",
            aggregate_id, cursor, batch_size
        );
        let remaining = budget.remaining()?;
        let batch =
            match tokio::time::timeout(remaining, fetch_events_since(&requester, request)).await {
                Ok(result) => result?,
                Err(_) => {
                    budget.exhausted = true;
                    bail!("historical sync exceeded total deadline");
                }
            };
        ensure!(
            batch.aggregate_id == aggregate_id,
            "sync peer returned aggregate {} while fetching {}",
            batch.aggregate_id,
            aggregate_id
        );
        ensure!(
            batch.events.len() <= batch_size,
            "sync peer returned {} events, exceeding requested batch size {}",
            batch.events.len(),
            batch_size
        );
        info!(
            "Batch received with {} events for aggregate={} cursor={}",
            batch.events.len(),
            aggregate_id,
            cursor
        );

        let page_bytes: usize = bincode::serialized_size(&batch.events)
            .context("failed to measure historical sync page")?
            .try_into()
            .context("historical sync page size does not fit usize")?;
        budget.record_page(batch.events.len(), page_bytes)?;
        all_events
            .try_reserve(batch.events.len())
            .map_err(|error| {
                anyhow::anyhow!("failed to reserve historical sync buffer: {error}")
            })?;
        all_events.extend(batch.events);

        match batch.next {
            BatchCursor::Done => break,
            BatchCursor::Next(next_cursor) => {
                ensure!(
                    next_cursor > cursor,
                    "sync peer returned non-advancing cursor {next_cursor} from {cursor}"
                );
                cursor = next_cursor;
            }
        }
    }

    info!("Batch is done returning {} events", all_events.len());

    Ok(all_events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct_requester::DirectRequesterTester;
    use crate::events::{NetCommand, NetEvent, PeerTarget};
    use std::sync::Arc;
    use tokio::sync::{broadcast, mpsc};

    #[test]
    fn sync_fetch_budget_rejects_each_resource_limit_without_mutating_state() {
        let mut budget = SyncFetchBudget::new(1, 2, 3);
        budget.record_page(2, 3).unwrap();

        let page_error = budget.record_page(0, 0).unwrap_err();
        assert!(page_error.to_string().contains("page limit"));
        assert_eq!((budget.pages, budget.events, budget.bytes), (1, 2, 3));

        let mut event_budget = SyncFetchBudget::new(2, 1, 3);
        let event_error = event_budget.record_page(2, 0).unwrap_err();
        assert!(event_error.to_string().contains("event limit"));
        assert_eq!(
            (event_budget.pages, event_budget.events, event_budget.bytes),
            (0, 0, 0)
        );

        let mut byte_budget = SyncFetchBudget::new(2, 2, 1);
        let byte_error = byte_budget.record_page(0, 2).unwrap_err();
        assert!(byte_error.to_string().contains("byte limit"));
        assert_eq!(
            (byte_budget.pages, byte_budget.events, byte_budget.bytes),
            (0, 0, 0)
        );
    }

    #[tokio::test]
    async fn test_fetch_all_batched_events() {
        let (net_cmds_tx, net_cmds_rx) = mpsc::channel::<NetCommand>(16);
        let (net_events_tx, net_events_rx) = broadcast::channel::<NetEvent>(16);
        let net_events = Arc::new(net_events_rx);

        let requester = DirectRequester::builder(net_cmds_tx, net_events).build();

        let batch1 = EventBatch {
            events: vec![b"event1".to_vec(), b"event2".to_vec()],
            next: BatchCursor::Next(100),
            aggregate_id: AggregateId::new(1),
        };
        let batch2 = EventBatch {
            events: vec![b"event3".to_vec()],
            next: BatchCursor::Done,
            aggregate_id: AggregateId::new(1),
        };

        let handle = DirectRequesterTester::new(net_cmds_rx, net_events_tx)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 0, 100))
            .respond_with(batch1)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 100, 100))
            .respond_with(batch2)
            .spawn();

        let events: Vec<Vec<u8>> =
            fetch_all_batched_events(requester, PeerTarget::Random, AggregateId::new(1), 0, 100)
                .await
                .unwrap();

        handle.await.unwrap();

        assert_eq!(
            events,
            vec![b"event1".to_vec(), b"event2".to_vec(), b"event3".to_vec(),]
        );
    }

    #[tokio::test]
    async fn test_no_duplicate_events_across_batches() {
        let (net_cmds_tx, net_cmds_rx) = mpsc::channel::<NetCommand>(16);
        let (net_events_tx, net_events_rx) = broadcast::channel::<NetEvent>(16);
        let net_events = Arc::new(net_events_rx);

        let requester = DirectRequester::builder(net_cmds_tx, net_events).build();

        let batch1 = EventBatch {
            events: vec![b"event1".to_vec(), b"event2".to_vec()],
            next: BatchCursor::Next(2),
            aggregate_id: AggregateId::new(1),
        };
        let batch2 = EventBatch {
            events: vec![b"event3".to_vec()],
            next: BatchCursor::Done,
            aggregate_id: AggregateId::new(1),
        };

        let handle = DirectRequesterTester::new(net_cmds_rx, net_events_tx)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 0, 100))
            .respond_with(batch1)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 2, 100))
            .respond_with(batch2)
            .spawn();

        let events: Vec<Vec<u8>> =
            fetch_all_batched_events(requester, PeerTarget::Random, AggregateId::new(1), 0, 100)
                .await
                .unwrap();

        handle.await.unwrap();

        let unique: Vec<_> = events
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(
            events.len(),
            unique.len(),
            "Found duplicate events: total={}, unique={}",
            events.len(),
            unique.len()
        );
        assert_eq!(
            events,
            vec![b"event1".to_vec(), b"event2".to_vec(), b"event3".to_vec()]
        );
    }

    #[tokio::test]
    async fn test_cursor_correctly_excludes_last_event_on_batch_boundary() {
        let (net_cmds_tx, net_cmds_rx) = mpsc::channel::<NetCommand>(16);
        let (net_events_tx, net_events_rx) = broadcast::channel::<NetEvent>(16);
        let net_events = Arc::new(net_events_rx);

        let requester = DirectRequester::builder(net_cmds_tx, net_events).build();

        let batch1 = EventBatch {
            events: vec![b"event1".to_vec()],
            next: BatchCursor::Next(100),
            aggregate_id: AggregateId::new(1),
        };
        let batch2 = EventBatch {
            events: vec![b"event2".to_vec()],
            next: BatchCursor::Done,
            aggregate_id: AggregateId::new(1),
        };

        let handle = DirectRequesterTester::new(net_cmds_rx, net_events_tx)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 0, 1))
            .respond_with(batch1)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 100, 1))
            .respond_with(batch2)
            .spawn();

        let events: Vec<Vec<u8>> =
            fetch_all_batched_events(requester, PeerTarget::Random, AggregateId::new(1), 0, 1)
                .await
                .unwrap();

        handle.await.unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0], b"event1");
        assert_eq!(events[1], b"event2");
    }

    #[tokio::test]
    async fn test_non_advancing_cursor_is_rejected() {
        let (net_cmds_tx, net_cmds_rx) = mpsc::channel::<NetCommand>(16);
        let (net_events_tx, net_events_rx) = broadcast::channel::<NetEvent>(16);
        let net_events = Arc::new(net_events_rx);

        let requester = DirectRequester::builder(net_cmds_tx, net_events).build();
        let batch = EventBatch {
            events: vec![b"event1".to_vec()],
            next: BatchCursor::Next(0),
            aggregate_id: AggregateId::new(1),
        };
        let handle = DirectRequesterTester::new(net_cmds_rx, net_events_tx)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 0, 1))
            .respond_with(batch)
            .spawn();

        let error = fetch_all_batched_events::<Vec<u8>>(
            requester,
            PeerTarget::Random,
            AggregateId::new(1),
            0,
            1,
        )
        .await
        .unwrap_err();

        handle.await.unwrap();
        assert!(error.to_string().contains("non-advancing cursor"));
    }

    #[tokio::test]
    async fn test_single_batch_with_limit_exactly_matched_returns_done() {
        let (net_cmds_tx, net_cmds_rx) = mpsc::channel::<NetCommand>(16);
        let (net_events_tx, net_events_rx) = broadcast::channel::<NetEvent>(16);
        let net_events = Arc::new(net_events_rx);

        let requester = DirectRequester::builder(net_cmds_tx, net_events).build();

        let batch = EventBatch {
            events: vec![b"event1".to_vec()],
            next: BatchCursor::Done,
            aggregate_id: AggregateId::new(1),
        };

        let handle = DirectRequesterTester::new(net_cmds_rx, net_events_tx)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 0, 1))
            .respond_with(batch)
            .spawn();

        let events: Vec<Vec<u8>> =
            fetch_all_batched_events(requester, PeerTarget::Random, AggregateId::new(1), 0, 1)
                .await
                .unwrap();

        handle.await.unwrap();

        assert_eq!(events, vec![b"event1".to_vec()]);
    }

    #[tokio::test]
    async fn test_three_batches_with_cursor_continuity() {
        let (net_cmds_tx, net_cmds_rx) = mpsc::channel::<NetCommand>(16);
        let (net_events_tx, net_events_rx) = broadcast::channel::<NetEvent>(16);
        let net_events = Arc::new(net_events_rx);

        let requester = DirectRequester::builder(net_cmds_tx, net_events).build();

        let batch1 = EventBatch {
            events: vec![b"a".to_vec(), b"b".to_vec()],
            next: BatchCursor::Next(200),
            aggregate_id: AggregateId::new(1),
        };
        let batch2 = EventBatch {
            events: vec![b"c".to_vec(), b"d".to_vec()],
            next: BatchCursor::Next(400),
            aggregate_id: AggregateId::new(1),
        };
        let batch3 = EventBatch {
            events: vec![b"e".to_vec()],
            next: BatchCursor::Done,
            aggregate_id: AggregateId::new(1),
        };

        let handle = DirectRequesterTester::new(net_cmds_rx, net_events_tx)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 0, 2))
            .respond_with(batch1)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 200, 2))
            .respond_with(batch2)
            .expect_request(FetchEventsSince::new(AggregateId::new(1), 400, 2))
            .respond_with(batch3)
            .spawn();

        let events: Vec<Vec<u8>> =
            fetch_all_batched_events(requester, PeerTarget::Random, AggregateId::new(1), 0, 2)
                .await
                .unwrap();

        handle.await.unwrap();

        let expected = vec![
            b"a".to_vec(),
            b"b".to_vec(),
            b"c".to_vec(),
            b"d".to_vec(),
            b"e".to_vec(),
        ];
        assert_eq!(events.len(), expected.len());
        assert_eq!(events, expected);
    }
}
