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
use anyhow::{bail, Context, Result};
use e3_data::Repositories;
use e3_events::{
    AggregateConfig, BusHandle, CorrelationId, EffectsEnabled, Event, EventPublisher,
    EventStoreQueryBy, EventStoreQueryResponse, EventSubscriber, EventType, EvmEventConfig,
    HistoricalEvmEventsReceived, HistoricalEvmSyncStart, HistoricalNetSyncStart, InterfoldEvent,
    InterfoldEventData, SeqAgg, StoreKeys, SyncEnded, Unsequenced,
};
#[cfg(test)]
use e3_events::{EventBusBarrier, EventBusFanout, EventContextAccessors};
use e3_utils::actix::channel as actix_toolbox;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tracing::info;

#[cfg(test)]
const REPLAY_PROGRESS_INTERVAL: usize = 10_000;

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
    // Note: this is *not* a global absence of restart recovery. Actors that hold determined,
    // idempotent in-flight results re-drive themselves when `EffectsEnabled` is broadcast at the
    // end of this sync (e.g. `ThresholdKeyshare::resume_in_flight_work` re-publishes a computed
    // keyshare / decryption share). What sync deliberately avoids is replaying *request* events.
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

/// Validate or initialize the durable schema marker before runtime actors can write state.
///
/// A fresh store is stamped synchronously; incompatible or unversioned protocol state halts. The
/// check is idempotent so composition roots may run it before actor construction while [`sync`]
/// retains the same guard for direct callers.
pub async fn preflight_schema_version(
    repositories: &Repositories,
    aggregate_config: &AggregateConfig,
    eventstore: &Recipient<EventStoreQueryBy<SeqAgg>>,
) -> Result<()> {
    let repo = repositories.schema_version();
    let persisted = repo.read().await?;
    let has_existing_state = if persisted.is_none() {
        has_schema_governed_kv_state(repositories).await?
            || event_logs_have_events(aggregate_config, eventstore).await?
    } else {
        false
    };
    match decide_schema_version(persisted, SCHEMA_VERSION, has_existing_state) {
        SchemaVersionDecision::Proceed => Ok(()),
        SchemaVersionDecision::WriteCurrent => {
            info!("Stamping on-disk schema version {SCHEMA_VERSION}.");
            repo.write_sync(&SCHEMA_VERSION).await?;
            Ok(())
        }
        SchemaVersionDecision::Halt(reason) => {
            bail!("Schema version check failed: {reason}");
        }
    }
}

/// Return whether the key/value store contains state whose interpretation requires a schema
/// marker.
///
/// Wallet setup runs before node startup and atomically writes the encrypted operator and libp2p
/// identities. That complete pair is the only pre-schema key/value state considered fresh. A
/// partial identity, any additional key, or any other non-empty key set remains schema-governed so
/// startup fails closed instead of asserting compatibility with unknown bytes.
pub async fn has_schema_governed_kv_state(repositories: &Repositories) -> Result<bool> {
    if repositories.store.is_empty().await? {
        return Ok(false);
    }

    let bootstrap_identity_keys = [StoreKeys::eth_private_key(), StoreKeys::libp2p_keypair()];
    Ok(!repositories
        .store
        .has_exact_keys(bootstrap_identity_keys)
        .await?)
}

async fn event_logs_have_events(
    aggregate_config: &AggregateConfig,
    eventstore: &Recipient<EventStoreQueryBy<SeqAgg>>,
) -> Result<bool> {
    let query = aggregate_config
        .aggregates()
        .into_iter()
        .map(|aggregate_id| (aggregate_id, 1))
        .collect();
    let (response, receiver) = actix_toolbox::oneshot::<EventStoreQueryResponse>();
    eventstore
        .send(EventStoreQueryBy::<SeqAgg>::new(CorrelationId::new(), query, response).with_limit(1))
        .await
        .context("event-store router stopped during schema preflight")?;
    Ok(!receiver
        .await
        .context("event-store query stopped during schema preflight")?
        .into_events()
        .is_empty())
}

pub async fn collect_historical_evm_events(
    mut receiver: Receiver<HistoricalEvmEventsReceived>,
    config: &EvmEventConfig,
) -> Result<Vec<InterfoldEvent<Unsequenced>>> {
    let mut collector = HistoricalEvmCollector::new(config);
    let progress_interval = Duration::from_secs(30);

    while !collector.is_complete() {
        match tokio::time::timeout(progress_interval, receiver.recv()).await {
            Ok(Some(mut msg)) => {
                let chain_id = msg.chain_id;
                if let CollectOutcome::Recorded {
                    chains_received,
                    chains_expected,
                } = collector.record(&mut msg)
                {
                    info!(
                        chain_id,
                        chains_received, chains_expected, "Received historical events from chain"
                    );
                }
            }
            Ok(None) => {
                let remaining = collector.remaining();
                bail!("historical EVM event channel closed before chains reported: {remaining:?}");
            }
            Err(_) => {
                // Not a failure — just a progress heartbeat
                let remaining = collector.remaining();
                info!(
                    ?remaining,
                    "Still waiting for historical events from chains"
                );
                continue;
            }
        }
    }

    Ok(collector.into_events())
}

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
mod tests {
    use super::{
        collect_historical_evm_events, has_schema_governed_kv_state, preflight_schema_version,
        publish_reconciled_history, replay_eventstore_events,
    };
    use crate::SyncRepositoryFactory;
    use e3_ciphernode_builder::EventSystem;
    use e3_data::Repositories;
    use e3_events::{
        hlc::{Hlc, HlcTimestamp},
        EffectsEnabled, Event, EventContextAccessors, EventPublisher, EventSubscriber, EventType,
        EvmEventConfig, EvmEventConfigChain, GetEvents, HistoricalEvmEventsReceived,
        HistoricalEvmSyncStart, InterfoldEvent, InterfoldEventData, StoreKeys, SyncEnded,
        TakeEvents, Unsequenced,
    };
    use e3_utils::MAILBOX_LIMIT_LARGE;
    use std::collections::BTreeMap;

    fn make_historical_evm_sync_start() -> HistoricalEvmSyncStart {
        HistoricalEvmSyncStart {
            evm_config: EvmEventConfig::new(),
            sender: None,
        }
    }

    fn evm_config(chains: &[u64]) -> EvmEventConfig {
        EvmEventConfig::from_config(
            chains
                .iter()
                .map(|chain_id| (*chain_id, EvmEventConfigChain::new(0)))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn historical_batch(chain_id: u64, event_count: usize) -> HistoricalEvmEventsReceived {
        let events = (0..event_count)
            .map(|index| {
                InterfoldEvent::<Unsequenced>::test_event("historical")
                    .id(index as u64 + 1)
                    .build()
            })
            .collect();
        HistoricalEvmEventsReceived::new(events, chain_id)
    }

    #[actix::test]
    async fn schema_preflight_rejects_unversioned_snapshot_state() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let store = system.store()?;
        store.scope("legacy").write_sync(7_u64).await?;
        let repositories = Repositories::from(&store);
        let eventstore = system.eventstore_reader()?.seq();

        let error =
            preflight_schema_version(&repositories, &system.aggregate_config(), &eventstore)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("no schema marker"));
        Ok(())
    }

    #[actix::test]
    async fn schema_preflight_initializes_store_with_only_bootstrap_identity() -> anyhow::Result<()>
    {
        let system = EventSystem::new().with_fresh_bus();
        let store = system.store()?;
        let private_key = vec![1_u8, 2, 3];
        let network_key = vec![4_u8, 5, 6];
        store
            .write_batch_sync([
                (StoreKeys::eth_private_key(), private_key.clone()),
                (StoreKeys::libp2p_keypair(), network_key.clone()),
            ])
            .await?;
        let repositories = Repositories::from(&store);
        let eventstore = system.eventstore_reader()?.seq();

        assert!(!has_schema_governed_kv_state(&repositories).await?);
        preflight_schema_version(&repositories, &system.aggregate_config(), &eventstore).await?;

        assert_eq!(
            repositories.schema_version().read().await?,
            Some(super::SCHEMA_VERSION)
        );
        assert_eq!(
            store
                .scope(StoreKeys::eth_private_key())
                .read::<Vec<u8>>()
                .await?,
            Some(private_key)
        );
        assert_eq!(
            store
                .scope(StoreKeys::libp2p_keypair())
                .read::<Vec<u8>>()
                .await?,
            Some(network_key)
        );
        Ok(())
    }

    #[actix::test]
    async fn schema_preflight_rejects_bootstrap_identity_plus_protocol_state() -> anyhow::Result<()>
    {
        let system = EventSystem::new().with_fresh_bus();
        let store = system.store()?;
        store
            .write_batch_sync([
                (StoreKeys::eth_private_key(), vec![1_u8]),
                (StoreKeys::libp2p_keypair(), vec![2_u8]),
            ])
            .await?;
        store
            .scope(StoreKeys::sortition())
            .write_sync(7_u64)
            .await?;
        let repositories = Repositories::from(&store);
        let eventstore = system.eventstore_reader()?.seq();

        assert!(has_schema_governed_kv_state(&repositories).await?);
        let error =
            preflight_schema_version(&repositories, &system.aggregate_config(), &eventstore)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("no schema marker"));
        assert_eq!(repositories.schema_version().read().await?, None);
        Ok(())
    }

    #[actix::test]
    async fn schema_preflight_rejects_partial_bootstrap_identity() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let store = system.store()?;
        store
            .scope(StoreKeys::eth_private_key())
            .write_sync(vec![1_u8])
            .await?;
        let repositories = Repositories::from(&store);
        let eventstore = system.eventstore_reader()?.seq();

        let error =
            preflight_schema_version(&repositories, &system.aggregate_config(), &eventstore)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("no schema marker"));
        assert_eq!(repositories.schema_version().read().await?, None);
        Ok(())
    }

    #[actix::test]
    async fn schema_preflight_rejects_unversioned_event_log() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let store = system.store()?;
        let repositories = Repositories::from(&store);
        let bus = system.handle()?.enable("unversioned-event-log");
        bus.publish_without_context(e3_events::TestEvent::new("legacy", 1))?;
        bus.flush_event_pipeline().await?;
        let eventstore = system.eventstore_reader()?.seq();

        let error =
            preflight_schema_version(&repositories, &system.aggregate_config(), &eventstore)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("no schema marker"));
        Ok(())
    }

    #[actix::test]
    async fn historical_evm_collection_fails_when_any_chain_disconnects() {
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        sender.send(historical_batch(1, 2)).await.unwrap();
        drop(sender);

        let error = collect_historical_evm_events(receiver, &evm_config(&[1, 2, 3]))
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "historical EVM event channel closed before chains reported: [2, 3]"
        );
    }

    #[actix::test]
    async fn historical_evm_collection_returns_only_after_every_chain_reports() {
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        sender.send(historical_batch(2, 3)).await.unwrap();
        sender.send(historical_batch(1, 2)).await.unwrap();

        let events = collect_historical_evm_events(receiver, &evm_config(&[1, 2]))
            .await
            .unwrap();

        assert_eq!(events.len(), 5);
    }

    #[actix::test]
    async fn infrastructure_events_are_filtered_during_replay() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-sync-replay");
        let history = bus.history();

        let events: Vec<InterfoldEvent> = vec![
            InterfoldEvent::<Unsequenced>::test_event("before")
                .id(1)
                .seq(1)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("sync")
                .data(SyncEnded::new())
                .seq(2)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("fx")
                .data(EffectsEnabled::new())
                .seq(3)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("evm")
                .data(make_historical_evm_sync_start())
                .seq(4)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("after")
                .id(2)
                .seq(5)
                .build(),
        ];

        let replayed = replay_eventstore_events(&bus, events).await?;
        assert_eq!(replayed, 2);

        let received = history.send(TakeEvents::new(2)).await?;

        let event_types: Vec<&'static str> = received
            .events
            .iter()
            .map(|e| match e.get_data() {
                InterfoldEventData::TestEvent(_) => "TestEvent",
                InterfoldEventData::SyncEnded(_) => "SyncEnded",
                InterfoldEventData::EffectsEnabled(_) => "EffectsEnabled",
                InterfoldEventData::HistoricalEvmSyncStart(_) => "HistoricalEvmSyncStart",
                _ => "other",
            })
            .collect();

        assert_eq!(event_types, vec!["TestEvent", "TestEvent"]);

        let msgs: Vec<String> = received
            .events
            .iter()
            .filter_map(|e| {
                if let InterfoldEventData::TestEvent(t) = e.get_data() {
                    Some(t.msg.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(msgs, vec!["before", "after"]);
        Ok(())
    }

    #[actix::test]
    async fn replay_backlog_larger_than_event_bus_mailbox_is_delivered() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-large-sync-replay");
        let history = bus.history();
        let count = MAILBOX_LIMIT_LARGE * 2;
        let events = (0..count)
            .map(|i| {
                InterfoldEvent::<Unsequenced>::test_event("replay")
                    .id(i as u64 + 1)
                    .seq(i as u64 + 1)
                    .build()
            })
            .collect();

        let replayed = replay_eventstore_events(&bus, events).await?;
        assert_eq!(replayed, count);

        let received = history.send(TakeEvents::new(count)).await?;
        assert!(!received.timed_out, "all replay events should be delivered");
        assert_eq!(received.events.len(), count);
        Ok(())
    }

    #[actix::test]
    async fn replay_restores_global_timestamp_order_across_aggregates() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-ordered-sync-replay");
        let history = bus.history();
        let events = vec![
            InterfoldEvent::<Unsequenced>::test_event("third")
                .id(3)
                .aggregate_id(3)
                .ts(30)
                .seq(1)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("first")
                .id(1)
                .aggregate_id(1)
                .ts(10)
                .seq(1)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("second")
                .id(2)
                .aggregate_id(2)
                .ts(20)
                .seq(1)
                .build(),
        ];

        replay_eventstore_events(&bus, events).await?;

        let received = history.send(TakeEvents::new(3)).await?;
        let timestamps = received
            .events
            .iter()
            .map(|event| event.ts())
            .collect::<Vec<_>>();
        assert_eq!(timestamps, vec![10, 20, 30]);
        Ok(())
    }

    #[actix::test]
    async fn replay_seeds_clock_from_post_snapshot_log_history() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system
            .handle()?
            .enable_with_hlc(Hlc::new(7).with_clock(|| 1_000));
        let durable = HlcTimestamp::new(5_000, 17, 99);
        let events = vec![InterfoldEvent::<Unsequenced>::test_event("durable")
            .id(1)
            .ts(durable.to_u128())
            .seq(1)
            .build()];

        replay_eventstore_events(&bus, events).await?;

        assert!(HlcTimestamp::from(bus.ts()?) > durable);
        Ok(())
    }

    #[actix::test]
    async fn startup_history_is_fenced_between_effects_and_live_mode() -> anyhow::Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-startup-history-fences");
        let history = bus.history();
        let historical = vec![
            InterfoldEvent::<Unsequenced>::test_event("first")
                .id(1)
                .ts(10)
                .build(),
            InterfoldEvent::<Unsequenced>::test_event("second")
                .id(2)
                .ts(20)
                .build(),
        ];

        publish_reconciled_history(&bus, historical).await?;

        let received = history.send(GetEvents::new()).await?;
        let types = received
            .iter()
            .map(|event| event.event_type())
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            ["EffectsEnabled", "TestEvent", "TestEvent", "SyncEnded"]
        );
        Ok(())
    }

    /// Verify that `run_once::<EffectsEnabled>` correctly gates event subscriptions.
    ///
    /// Simulates the sync flow:
    /// 1. An event is published BEFORE EffectsEnabled (should be dropped — nobody listening)
    /// 2. EffectsEnabled is published (triggers subscription)
    /// 3. The same event is published AFTER EffectsEnabled (should be received)
    ///
    /// This is the pattern used by Sortition (E3Requested), CommitteeFinalizer
    /// (CommitteeRequested), Multithread (ComputeRequest), and the sol writers.
    #[actix::test]
    async fn effects_enabled_gates_event_subscriptions() -> anyhow::Result<()> {
        use std::sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        };

        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-effects-gating");

        let receive_count = Arc::new(AtomicU32::new(0));

        // Set up a gated subscription: only subscribe to TestEvent after EffectsEnabled
        let counter = receive_count.clone();
        let runner = e3_events::run_once::<EffectsEnabled>({
            let bus = bus.clone();
            move |_| {
                // Create a simple actor that counts received TestEvents
                use actix::{Actor, Context, Handler};

                struct Counter(Arc<AtomicU32>);
                impl Actor for Counter {
                    type Context = Context<Self>;
                }
                impl Handler<InterfoldEvent> for Counter {
                    type Result = ();
                    fn handle(
                        &mut self,
                        msg: InterfoldEvent,
                        _: &mut Self::Context,
                    ) -> Self::Result {
                        if matches!(msg.get_data(), InterfoldEventData::TestEvent(_)) {
                            self.0.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }

                let addr = Counter(counter).start();
                bus.subscribe(EventType::TestEvent, addr.recipient());
                Ok(())
            }
        });
        bus.subscribe(EventType::EffectsEnabled, runner.recipient());

        // 1. Publish a TestEvent BEFORE EffectsEnabled — should NOT be received
        bus.event_bus().try_send(
            InterfoldEvent::<Unsequenced>::test_event("before-effects")
                .id(1)
                .seq(1)
                .build(),
        )?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            receive_count.load(Ordering::SeqCst),
            0,
            "Event before EffectsEnabled should not be received"
        );

        // 2. Publish EffectsEnabled — triggers the subscription
        bus.publish_without_context(EffectsEnabled::new())?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. Publish a TestEvent AFTER EffectsEnabled — should be received
        bus.event_bus().try_send(
            InterfoldEvent::<Unsequenced>::test_event("after-effects")
                .id(2)
                .seq(2)
                .build(),
        )?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            receive_count.load(Ordering::SeqCst),
            1,
            "Event after EffectsEnabled should be received exactly once"
        );

        Ok(())
    }

    /// Verify that ungated (immediate) subscriptions receive events both
    /// before and after EffectsEnabled.
    ///
    /// This mirrors how Sortition subscribes to state-building events
    /// (CiphernodeAdded, E3Failed, etc.) immediately, while gating
    /// E3Requested behind EffectsEnabled. The immediate subscriptions
    /// must work during EventStore replay (before EffectsEnabled).
    #[actix::test]
    async fn immediate_subscriptions_receive_before_effects_enabled() -> anyhow::Result<()> {
        use std::sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        };

        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-immediate-sub");

        let immediate_count = Arc::new(AtomicU32::new(0));
        let gated_count = Arc::new(AtomicU32::new(0));

        // Helper actor that counts TestEvents
        use actix::{Actor, Context, Handler};

        struct Counter(Arc<AtomicU32>);
        impl Actor for Counter {
            type Context = Context<Self>;
        }
        impl Handler<InterfoldEvent> for Counter {
            type Result = ();
            fn handle(&mut self, msg: InterfoldEvent, _: &mut Self::Context) -> Self::Result {
                if matches!(msg.get_data(), InterfoldEventData::TestEvent(_)) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        // Immediate subscription — receives all events, including before EffectsEnabled
        let immediate_actor = Counter(immediate_count.clone()).start();
        bus.subscribe(EventType::TestEvent, immediate_actor.recipient());

        // Gated subscription — only receives after EffectsEnabled
        let gated_counter = gated_count.clone();
        let runner = e3_events::run_once::<EffectsEnabled>({
            let bus = bus.clone();
            move |_| {
                let addr = Counter(gated_counter).start();
                bus.subscribe(EventType::TestEvent, addr.recipient());
                Ok(())
            }
        });
        bus.subscribe(EventType::EffectsEnabled, runner.recipient());

        // 1. Publish event BEFORE EffectsEnabled
        bus.event_bus().try_send(
            InterfoldEvent::<Unsequenced>::test_event("during-replay")
                .id(1)
                .seq(1)
                .build(),
        )?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            immediate_count.load(Ordering::SeqCst),
            1,
            "Immediate subscription should receive events before EffectsEnabled"
        );
        assert_eq!(
            gated_count.load(Ordering::SeqCst),
            0,
            "Gated subscription should NOT receive events before EffectsEnabled"
        );

        // 2. Publish EffectsEnabled
        bus.publish_without_context(EffectsEnabled::new())?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. Publish event AFTER EffectsEnabled
        bus.event_bus().try_send(
            InterfoldEvent::<Unsequenced>::test_event("after-effects")
                .id(2)
                .seq(2)
                .build(),
        )?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            immediate_count.load(Ordering::SeqCst),
            2,
            "Immediate subscription should receive events after EffectsEnabled too"
        );
        assert_eq!(
            gated_count.load(Ordering::SeqCst),
            1,
            "Gated subscription should receive events after EffectsEnabled"
        );

        Ok(())
    }
}
