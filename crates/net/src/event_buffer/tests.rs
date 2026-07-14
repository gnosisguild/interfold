// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::time::Duration;

use super::*;
use crate::events::{GossipData, NetEvent};
use e3_ciphernode_builder::EventSystem;
use e3_events::EventPublisher;
use e3_events::SyncEnded;
use tokio::{
    sync::broadcast,
    time::{sleep, timeout},
};

#[actix::test]
async fn test_buffers_until_sync_ended() -> Result<()> {
    // Setup
    let system = EventSystem::new().with_fresh_bus();
    let bus = system.handle()?.enable("test");
    let (input_tx, input_rx) = broadcast::channel(16);
    let (mut output_rx, handle) = NetEventBuffer::setup_with_limits(
        &bus,
        &input_rx,
        DEFAULT_MAX_BUFFERED_NET_EVENTS,
        DEFAULT_MAX_BUFFERED_NET_BYTES,
    );

    // Send events while syncing - should be buffered
    let event1 = NetEvent::GossipData(GossipData::GossipBytes(vec![1, 2, 3]));
    let event2 = NetEvent::GossipData(GossipData::GossipBytes(vec![4, 5, 6]));
    input_tx.send(event1.clone()).unwrap();
    input_tx.send(event2.clone()).unwrap();

    // Give actor time to process
    sleep(Duration::from_millis(10)).await;

    // Verify no events forwarded yet (should timeout)
    assert!(
        timeout(Duration::from_millis(50), output_rx.recv())
            .await
            .is_err(),
        "Events should be buffered, not forwarded during sync"
    );

    // Send SyncEnded event
    bus.publish_without_context(SyncEnded::new()).unwrap();
    handle.wait_until_running().await?;

    // Now buffered events should be forwarded
    let received1 = output_rx.recv().await.unwrap();
    let received2 = output_rx.recv().await.unwrap();

    assert!(
        matches!(received1, NetEvent::GossipData(GossipData::GossipBytes(ref bytes)) if bytes == &vec![1, 2, 3])
    );
    assert!(
        matches!(received2, NetEvent::GossipData(GossipData::GossipBytes(ref bytes)) if bytes == &vec![4, 5, 6])
    );

    // Send new event after sync - should forward immediately
    let event3 = NetEvent::GossipData(GossipData::GossipBytes(vec![7, 8, 9]));
    input_tx.send(event3.clone()).unwrap();

    let received3 = tokio::time::timeout(tokio::time::Duration::from_millis(100), output_rx.recv())
        .await
        .expect("Event should be forwarded immediately after sync")
        .unwrap();

    assert!(
        matches!(received3, NetEvent::GossipData(GossipData::GossipBytes(ref bytes)) if bytes == &vec![7, 8, 9])
    );

    Ok(())
}

#[actix::test]
async fn startup_buffer_overflow_fails_readiness_without_dropping_oldest() -> Result<()> {
    let system = EventSystem::new().with_fresh_bus();
    let bus = system.handle()?.enable("test-overflow");
    let (input_tx, input_rx) = broadcast::channel(16);
    let (_output_rx, handle) =
        NetEventBuffer::setup_with_limits(&bus, &input_rx, 1, DEFAULT_MAX_BUFFERED_NET_BYTES);

    input_tx.send(NetEvent::GossipData(GossipData::GossipBytes(vec![1])))?;
    input_tx.send(NetEvent::GossipData(GossipData::GossipBytes(vec![2])))?;

    let error = timeout(Duration::from_secs(1), handle.wait_until_running())
        .await
        .context("network buffer did not report overflow")?
        .expect_err("overflow must fail startup readiness")
        .to_string();
    assert!(error.contains("events=1/1"), "{error}");
    assert!(
        error.contains("startup will stop rather than drop"),
        "{error}"
    );
    Ok(())
}

#[actix::test]
async fn startup_buffer_enforces_estimated_payload_bytes() -> Result<()> {
    let system = EventSystem::new().with_fresh_bus();
    let bus = system.handle()?.enable("test-byte-overflow");
    let (input_tx, input_rx) = broadcast::channel(16);
    let event = NetEvent::GossipData(GossipData::GossipBytes(vec![0; 32]));
    let estimated_bytes = event.buffered_size_bytes();
    let (_output_rx, handle) =
        NetEventBuffer::setup_with_limits(&bus, &input_rx, 16, estimated_bytes - 1);

    input_tx.send(event)?;

    let error = timeout(Duration::from_secs(1), handle.wait_until_running())
        .await
        .context("network buffer did not report byte overflow")?
        .expect_err("byte overflow must fail startup readiness")
        .to_string();
    assert!(
        error.contains(&format!("next_event_bytes={estimated_bytes}")),
        "{error}"
    );
    Ok(())
}
