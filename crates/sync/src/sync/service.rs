// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

#[cfg(test)]
use crate::domain::ReplayDecision;
use crate::domain::{
    decide_schema_version, CollectOutcome, HistoricalEvmCollector, SchemaVersionDecision,
    SnapshotMeta, SyncPlanner, SCHEMA_VERSION,
};
use crate::replay_spool::ReplaySpool;
use crate::SyncRepositoryFactory;
use actix::{Message, Recipient};
use anyhow::{bail, ensure, Context, Result};
use e3_data::Repositories;
use e3_events::{
    AggregateConfig, AggregateId, BusHandle, CorrelationId, EffectsEnabled, Event, EventPublisher,
    EventStoreQueryBy, EventStoreQueryResponse, EventSubscriber, EventType, EvmEventConfig,
    HistoricalEvmEventsReceived, HistoricalEvmSyncStart, HistoricalNetSyncStart, InterfoldEvent,
    InterfoldEventData, RequestRouterCheckpoint, SeqAgg, StoreKeys, SyncEnded, Unsequenced,
};
#[cfg(test)]
use e3_events::{EventBusBarrier, EventBusFanout, EventContextAccessors};
use e3_utils::actix::channel as actix_toolbox;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tracing::info;

#[cfg(test)]
const REPLAY_PROGRESS_INTERVAL: usize = 10_000;

/// Rebuild the request-router checkpoint when its cursors differ from aggregate snapshots.
///
/// The rebuild projects EventStore history only into router admission state. It does not replay
/// protocol actors, which already hydrate from their aggregate snapshots.
pub async fn reconcile_request_router_checkpoint(
    repositories: &Repositories,
    aggregate_ids: impl IntoIterator<Item = AggregateId>,
    eventstore: &Recipient<EventStoreQueryBy<SeqAgg>>,
) -> Result<()> {
    let mut target_cursors = std::collections::HashMap::new();
    for aggregate_id in aggregate_ids {
        let cursor = repositories
            .aggregate_seq(aggregate_id)
            .read()
            .await?
            .unwrap_or(0);
        target_cursors.insert(aggregate_id, cursor);
    }

    let checkpoint_store = repositories.request_router_checkpoint();
    let checkpoint = checkpoint_store
        .read()
        .await?
        .context("request-router checkpoint is missing after storage preflight")?;
    if checkpoint.replay_cursors == target_cursors {
        return Ok(());
    }

    info!(
        checkpoint_cursors = ?checkpoint.replay_cursors,
        target_cursors = ?target_cursors,
        "Rebuilding the request-router checkpoint from EventStore"
    );
    let spool = ReplaySpool::load_bounded(eventstore, target_cursors.clone()).await?;
    let mut rebuilt = RequestRouterCheckpoint {
        replay_cursors: target_cursors
            .keys()
            .copied()
            .map(|aggregate_id| (aggregate_id, 0))
            .collect(),
        ..Default::default()
    };
    let projected = spool.project(|event| {
        e3_request::project_request_router_event(&mut rebuilt, event);
        Ok(())
    })?;
    ensure!(
        rebuilt.replay_cursors == target_cursors,
        "request-router rebuild produced cursors {:?}, but aggregate snapshots require {:?}",
        rebuilt.replay_cursors,
        target_cursors
    );
    checkpoint_store.write_sync(&rebuilt).await?;
    info!(
        projected_events = projected,
        "Request-router checkpoint rebuilt"
    );
    Ok(())
}

pub async fn sync(
    bus: &BusHandle,
    default_config: &EvmEventConfig,
    repositories: &Repositories,
    aggregate_config: &AggregateConfig,
    eventstore: &Recipient<EventStoreQueryBy<SeqAgg>>,
) -> Result<()> {
    // 0. start listening early for net ready
    let net_ready = bus.wait_for(EventType::NetReady);

    // 0b. Verify the on-disk schema version is compatible with this binary
    //     before touching any persisted state, so an incompatible upgrade or
    //     downgrade halts loudly instead of silently loading garbage (H19/H20).
    preflight_schema_version(repositories, aggregate_config, eventstore).await?;

    // 1. Load snapsshot metadata
    info!("Loading snapshot metadata...");
    let snapshot =
        SnapshotMeta::read_from_disk(aggregate_config.aggregates(), default_config, repositories)
            .await?;
    info!(
        "Snapshot metadata loaded for {} aggregates.",
        snapshot.aggregates().len()
    );

    // 1b. Restore the HLC ordering floor from the highest persisted aggregate
    //     timestamp so events created after this restart remain strictly after
    //     durable history, including its logical counter, even if wall time moved backwards.
    if let Some(max_ts) = snapshot.to_net_config().values().copied().max() {
        bus.seed_clock(max_ts)?;
    }

    // 2. Determine the evm blocks to read from based on the SnapshotMeta
    let evm_config = snapshot.to_evm_config();
    let snapshot_net_config = snapshot.to_net_config();

    // 3. Page post-snapshot EventStore history into sorted temporary runs. This preserves the
    // global HLC replay order without retaining the complete backlog in memory.
    info!("Loading EventStore replay pages...");
    let request_router_checkpoint = repositories
        .request_router_checkpoint()
        .read()
        .await?
        .context("request-router recovery checkpoint is missing after storage preflight")?;
    snapshot.ensure_request_router_covers(&request_router_checkpoint.replay_cursors)?;
    let replay_spool = ReplaySpool::load(eventstore, snapshot.to_sequence_map()).await?;
    info!("{} EventStore events spooled.", replay_spool.total_events());

    info!("Replaying events to actors...");
    // 4. Replay the EventStore events to all listeners (except effects).
    //    Skip lifecycle infrastructure events. SyncEnded, EffectsEnabled and sync-start events are
    //    re-published by this sync process; Shutdown belongs to the previous process and would stop
    //    freshly constructed actors. Replaying these here
    //    would poison the EventBus deduplication window: the replayed event has the same
    //    EventId (payload hash) as the one we publish later, causing the later event to be
    //    silently dropped.  This is critical for SyncEnded, if the EvmChainGateway never
    //    receives it, the gateway stays in BufferUntilLive and all live EVM events are lost.
    let replayed = replay_spool.replay(bus).await?;
    info!(replayed_events = replayed, "Events replayed.");

    // Loose ends after a crash:
    //
    // Terminal E3 work that *completed while this node was down* is recovered by the
    // historical EVM re-fetch in step 5 below: the terminal on-chain events
    // (PlaintextOutputPublished / E3Failed / committee completion) are re-delivered once
    // effects are enabled, which re-drives the Sortition release path and frees any tickets
    // the node was still holding. So "an E3 finished while we were offline" needs no special
    // handling here — it is reconciled by replaying the canonical chain state.
    //
    // What is intentionally NOT auto-re-driven *here in sync* is this node's *own* in-flight
    // request work by replaying the originating request events. Blindly re-publishing the
    // originating request event is a no-op: the event bus dedups by EventId (payload hash), so
    // the replayed event is dropped. Forcibly minting a fresh EventId to force re-execution is
    // unsafe on a value-bearing protocol (it can double-emit or race the canonical chain state)
    // and is therefore deliberately left out of the sync path.
    //
    // Note: this is *not* a global absence of restart recovery. Per-E3 actors restore versioned
    // recovery inputs and re-create collectors, proof jobs, compute jobs, and determined outputs
    // when `EffectsEnabled` is broadcast at the end of this sync. What sync deliberately avoids is
    // replaying request prefixes into actors that already hydrated from protocol snapshots.
    //
    // Detection of loose ends that cannot be locally re-driven is exposed offline and
    // non-destructively via `interfold node validate`, which cross-checks the persisted committee
    // slots against terminal events in the log and reports orphaned tickets. See
    // `crates/entrypoint/src/validate.rs`.

    // 5. Load the historical evm events to memory from all chains
    info!("Loading historical blockchain events...");
    let (addr, rx) = actix_toolbox::mpsc::<HistoricalEvmEventsReceived>(256);
    bus.publish_without_context(HistoricalEvmSyncStart::new(addr, evm_config.clone()))?;
    let historical_evm_events = collect_historical_evm_events(rx, &evm_config).await?;
    info!(
        "{} historical blockchain events loaded.",
        historical_evm_events.len()
    );
    // Build the net sync cursor using snapshot timestamps (the original HLC timestamps
    // from before the restart). See SyncPlanner::net_sync_cursor for why the re-read EVM
    // event timestamps cannot be used.
    let net_config = SyncPlanner::net_sync_cursor(&historical_evm_events, &snapshot_net_config);

    // 6. Load the historical libp2p events to memory
    info!("Waiting until NetReady...");
    net_ready.await?;
    info!("NetReady!");
    info!("Loading historical libp2p events...");
    let events_received = bus.wait_for(EventType::HistoricalNetSyncEventsReceived);
    bus.publish_without_context(HistoricalNetSyncStart::new(net_config.clone()))?;
    let InterfoldEventData::HistoricalNetSyncEventsReceived(event) =
        events_received.await?.into_data()
    else {
        bail!("failed to get HistoricalNetSyncEventsReceived");
    };
    let historical_net_events = event.events;
    info!(
        "{} historical libp2p events loaded.",
        historical_net_events.len()
    );

    // 7. Sort both the evm and libp2p events together by HLC timestamp
    let mut historical = historical_evm_events
        .into_iter()
        .chain(historical_net_events)
        .collect::<Vec<_>>();

    SyncPlanner::sort_by_timestamp(&mut historical);
    info!("Historical events sorted.");

    // 8-10. Enable effects, publish canonical history, then enter live mode. Each phase is fenced
    // through durable storage and EventBus fanout so aggregate-specific EventStore response order
    // cannot move history ahead of EffectsEnabled or SyncEnded ahead of history.
    publish_reconciled_history(bus, historical).await?;
    // normal live operations

    Ok(())
}

async fn publish_reconciled_history(
    bus: &BusHandle,
    historical: Vec<InterfoldEvent<Unsequenced>>,
) -> Result<()> {
    info!("Enabling effects...");
    bus.publish_without_context(EffectsEnabled::new())?;
    bus.flush_event_pipeline().await?;
    info!("Effects enabled.");

    info!("Publishing historical events to actors...");
    for event in historical {
        bus.naked_dispatch_async(event).await?;
    }
    bus.flush_event_pipeline().await?;
    info!("Historical events published.");

    info!("Publishing SyncEnded event...");
    bus.publish_without_context(SyncEnded::new())?;
    bus.flush_event_pipeline().await?;
    info!("Sync finished.");
    Ok(())
}

#[cfg(test)]
async fn replay_eventstore_events(
    bus: &BusHandle,
    mut events: Vec<InterfoldEvent>,
) -> Result<usize> {
    let total_events = events.len();
    let mut replayed = 0usize;

    // Snapshot metadata can lag the append-only log after a failed snapshot write. Seed from the
    // actual replay set before any subscriber can emit follow-up work, otherwise new local events
    // may receive timestamps behind durable post-snapshot history.
    if let Some(max_ts) = events.iter().map(EventContextAccessors::ts).max() {
        bus.seed_clock(max_ts)?;
    }

    // EventStoreRouter gathers one query response per aggregate. Those actor responses can arrive
    // in any order, so replay must re-establish the global HLC order before stateful subscribers
    // observe cross-aggregate dependencies.
    events.sort_by_key(|event| event.ts());

    for event in events {
        if SyncPlanner::classify_replay(&event) == ReplayDecision::SkipInfrastructure {
            continue;
        }
        // Await EventBus handling before submitting the next event. `try_send` lets this producer
        // outrun the bounded mailbox and aborts startup when it fills; the awaited Actix request
        // preserves replay order, yields between events, and reports a closed mailbox.
        bus.event_bus().send(EventBusFanout(event)).await??;
        replayed += 1;

        if replayed.is_multiple_of(REPLAY_PROGRESS_INTERVAL) {
            info!(
                replayed_events = replayed,
                total_events, "EventStore replay progress"
            );
        }
    }
    // The EventBus acknowledges its own handler before an awaited subscriber fanout finishes.
    // A fence queued after the final replay event therefore proves the last downstream handler
    // has completed before startup advances to canonical-chain reconciliation.
    bus.event_bus().send(EventBusBarrier).await?;
    Ok(replayed)
}

#[path = "history.rs"]
mod historical;
mod preflight;

pub use historical::collect_historical_evm_events;
pub use preflight::{has_schema_governed_kv_state, preflight_schema_version};

#[derive(Message)]
#[rtype("()")]
pub struct Bootstrap;

#[derive(Message)]
#[rtype("()")]
pub struct SnapshotLoaded {
    pub snapshot: SnapshotMeta,
}
impl SnapshotLoaded {
    pub fn new(snapshot: SnapshotMeta) -> Self {
        Self { snapshot }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
