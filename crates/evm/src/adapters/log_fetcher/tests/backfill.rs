// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::*;

#[actix::test]
async fn test_backfill_no_gap() {
    let mock = MockLogProvider::new(100);
    let (next, _rx) = setup_collector();
    let mut ts = TimestampTracker::new();
    let filter = Filter::new();
    let mut last_block = 100u64;

    let result = backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 0).await;

    assert!(result.is_ok());
    assert_eq!(last_block, 100);
    assert_eq!(mock.get_logs_call_count(), 0);
}

#[actix::test]
async fn test_backfill_with_gap() {
    let mock = MockLogProvider::new(200);
    mock.push_logs(vec![make_test_log(150), make_test_log(180)]);
    let (next, mut rx) = setup_collector();
    let mut ts = TimestampTracker::new();
    let filter = Filter::new();
    let mut last_block = 100u64;

    let result = backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 0).await;

    assert!(result.is_ok());
    assert_eq!(last_block, 200);
    assert_eq!(mock.get_logs_call_count(), 1);

    tokio::task::yield_now().await;
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    assert_eq!(count, 2);
}

#[actix::test]
async fn test_backfill_partial_failure_preserves_progress() {
    tokio::time::pause();

    // Head at 25000, last_block at 100
    // Gap: blocks 101..=25000 → 3 chunks:
    //   chunk 1: [101, 10100]
    //   chunk 2: [10101, 20100]
    //   chunk 3: [20101, 25000]
    let mock = MockLogProvider::new(25000);
    // Chunk 1 succeeds
    mock.push_logs(vec![make_test_log(500)]);
    // Chunk 2 succeeds
    mock.push_logs(vec![make_test_log(15000)]);
    // Chunk 3: all retries fail
    for _ in 0..GET_LOGS_MAX_RETRIES {
        mock.push_error("RPC error");
    }

    let (next, _rx) = setup_collector();
    let mut ts = TimestampTracker::new();
    let filter = Filter::new();
    let mut last_block = 100u64;

    let result = backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 0).await;

    // Should fail because chunk 3 exhausted retries
    assert!(result.is_err());
    // But last_block must have advanced past the two successful chunks
    assert_eq!(last_block, 20100);

    // On retry: gap_start = 20101, head still 25000 → single chunk succeeds
    mock.push_logs(vec![make_test_log(22000)]);

    let result = backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 0).await;
    assert!(result.is_ok());
    assert_eq!(last_block, 25000);
}

#[actix::test]
async fn test_backfill_clamps_to_confirmed_head() {
    // Head at 200, but require 12 confirmations => only ingest up to 188.
    let mock = MockLogProvider::new(200);
    mock.push_logs(vec![make_test_log(150)]);
    let (next, _rx) = setup_collector();
    let mut ts = TimestampTracker::new();
    let filter = Filter::new();
    let mut last_block = 100u64;

    let result = backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 12).await;

    assert!(result.is_ok());
    // Advanced only to the confirmed head, not the raw head of 200.
    assert_eq!(last_block, 188);
}

#[actix::test]
async fn live_log_waits_for_confirmed_canonical_backfill() -> anyhow::Result<()> {
    let mock = MockLogProvider::new(200);
    let (next, mut rx) = setup_collector();
    let mut ts = TimestampTracker::new();
    let filter = Filter::new();
    let mut last_block = 188;

    let delivered = process_live_log(
        &mock,
        make_test_log(200),
        1,
        &next,
        &mut ts,
        &mut last_block,
        12,
    )
    .await?;
    assert!(delivered.is_none());
    assert_eq!(last_block, 188);
    tokio::task::yield_now().await;
    assert!(
        rx.try_recv().is_err(),
        "unconfirmed log must not be emitted"
    );

    mock.set_block_number(211);
    mock.push_logs(Vec::new());
    backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 12).await?;
    assert_eq!(last_block, 199);
    tokio::task::yield_now().await;
    assert!(rx.try_recv().is_err(), "eleven blocks is not enough");

    mock.set_block_number(212);
    mock.push_logs(vec![make_test_log(200)]);
    backfill_to_head(&mock, &filter, 1, &next, &mut ts, &mut last_block, 12).await?;
    assert_eq!(last_block, 200);
    let emitted = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await?
        .expect("confirmed log should be emitted");
    assert!(matches!(emitted, InterfoldEvmEvent::Log(_)));
    Ok(())
}
