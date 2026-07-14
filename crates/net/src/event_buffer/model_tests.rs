// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

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
