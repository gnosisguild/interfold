// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::{Actor, Addr, AsyncContext, Handler, Message, Recipient, ResponseFuture};
use anyhow::{bail, Context, Result};
use e3_events::{
    prelude::*, trap, trap_fut, AggregateId, BusHandle, CorrelationId, EType, EventSource,
    EventStoreFilter, EventStoreQueryBy, EventStoreQueryResponse, EventType,
    HistoricalNetSyncEventsReceived, HistoricalNetSyncStart, InterfoldEvent, InterfoldEventData,
    NetReady, TsAgg, TypedEvent, Unsequenced,
};
use e3_utils::MAILBOX_LIMIT;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::{
    direct_requester::DirectRequester,
    direct_responder::DirectResponder,
    domain::{
        build_sync_batch,
        net_event_batch::{
            fetch_all_batched_events_with_budget, FetchEventsSince, SyncFetchBudget,
        },
        sync_coordinator::sync_scan_limit,
        EventTranslationService, NetReadiness, ReadinessDecision, SyncBatchOutcome,
    },
    events::{
        await_event, GossipData, IncomingRequest, NetCommand, NetEvent, PeerTarget,
        ProtocolResponse,
    },
};

/// Maximum time to wait for a `ConnectionEstablished` event after all dials
/// failed before publishing `NetReady` anyway.
const NET_READY_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

/// Direct-request retry settings for a single historical sync fetch attempt.
const SYNC_FETCH_MAX_RETRIES: u32 = 3;
const SYNC_FETCH_RETRY_TIMEOUT: Duration = Duration::from_secs(5);

/// If a historical sync fetch fails, wait this long for a fresh connection
/// before retrying anyway against currently connected peers.
const SYNC_RECOVERY_RETRY_INTERVAL: Duration = Duration::from_secs(15);

/// Number of recovery rounds to try for failed aggregates after the initial fetch pass.
const SYNC_RECOVERY_MAX_ATTEMPTS: usize = 3;

/// Bound remote work independently of the actor mailbox. Per-peer admission prevents one
/// authenticated transport identity from occupying the global allowance.
const MAX_IN_FLIGHT_SYNC_REQUESTS: usize = 16;
const MAX_IN_FLIGHT_SYNC_REQUESTS_PER_PEER: usize = 2;

/// Expire storage requests before libp2p's 30-second request timeout so a failed local query cannot
/// retain a responder and permanently consume one of the bounded in-flight slots.
const INCOMING_SYNC_REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponseValue {
    pub events: Vec<InterfoldEvent<Unsequenced>>,
    pub ts: u128,
}

impl TryInto<Vec<u8>> for SyncResponseValue {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        bincode::serialize(&self).context("failed to serialize sync response")
    }
}

impl TryFrom<Vec<u8>> for SyncResponseValue {
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        bincode::deserialize(&value).context("failed to deserialize sync response")
    }
}

#[derive(Debug, Clone)]
pub struct SyncRequestSucceeded {
    pub response: SyncResponseValue,
}

struct PendingSyncRequest {
    peer: PeerId,
    responder: DirectResponder,
}

pub struct NetSyncManager {
    /// Interfold EventBus
    bus: BusHandle,
    /// NetCommand sender to forward commands to the Libp2pNetInterface
    tx: mpsc::Sender<NetCommand>,
    /// NetEvents receiver to receive events
    rx: Arc<broadcast::Receiver<NetEvent>>,
    eventstore: Recipient<EventStoreQueryBy<TsAgg>>,
    requests: HashMap<CorrelationId, PendingSyncRequest>,
    /// Pure readiness state machine.
    readiness: NetReadiness,
    /// Gossipsub topic used to re-broadcast our own forwardable artifacts after a restart.
    topic: String,
    /// Snapshot-cursor map captured from `HistoricalNetSyncStart`. Bounds the post-restart
    /// re-broadcast query to the in-flight (un-snapshotted) window.
    rebroadcast_since: Option<HashMap<AggregateId, u128>>,
    /// Correlation ids of in-flight re-broadcast EventStore queries, so their responses can be
    /// distinguished from ordinary sync-request responses.
    rebroadcast_query_ids: HashSet<CorrelationId>,
    /// Set once `NetReady` has been published (peers connected or fallback timeout elapsed).
    net_ready: bool,
    /// Guard so the post-restart re-broadcast fires at most once per process.
    rebroadcast_started: bool,
}

impl NetSyncManager {
    pub fn new(
        bus: &BusHandle,
        tx: &mpsc::Sender<NetCommand>,
        rx: &Arc<broadcast::Receiver<NetEvent>>,
        eventstore: Recipient<EventStoreQueryBy<TsAgg>>,
        topic: &str,
    ) -> Self {
        Self {
            bus: bus.clone(),
            tx: tx.clone(),
            rx: Arc::clone(rx),
            eventstore,
            requests: HashMap::new(),
            readiness: NetReadiness::new(),
            topic: topic.to_string(),
            rebroadcast_since: None,
            rebroadcast_query_ids: HashSet::new(),
            net_ready: false,
            rebroadcast_started: false,
        }
    }

    fn publish_net_ready(&self) -> Result<()> {
        info!("NetSyncManager: publishing NetReady");
        self.bus.publish_without_context(NetReady::new())?;
        Ok(())
    }

    fn request_capacity_error(&self, peer: &PeerId) -> Option<&'static str> {
        if self.requests.len() >= MAX_IN_FLIGHT_SYNC_REQUESTS {
            return Some("too many in-flight sync requests");
        }
        let peer_requests = self
            .requests
            .values()
            .filter(|request| &request.peer == peer)
            .count();
        if peer_requests >= MAX_IN_FLIGHT_SYNC_REQUESTS_PER_PEER {
            return Some("too many in-flight sync requests from this peer");
        }
        None
    }

    fn expire_sync_request(&mut self, id: CorrelationId) {
        let Some(pending) = self.requests.remove(&id) else {
            return;
        };
        warn!(
            peer = %pending.peer,
            correlation_id = %id,
            timeout_ms = INCOMING_SYNC_REQUEST_TIMEOUT.as_millis(),
            "Incoming historical-sync storage query timed out"
        );
        if let Err(error) = pending.responder.respond(ProtocolResponse::Error(
            "historical sync request timed out".to_string(),
        )) {
            warn!(
                peer = %pending.peer,
                correlation_id = %id,
                %error,
                "Failed to send historical-sync timeout response"
            );
        }
    }

    /// After a restart, proactively re-gossip this node's own already-produced forwardable DKG
    /// artifacts (H3/H11). Resume from a persisted phase is otherwise passive: the restored
    /// keyshare/aggregator actors wait for peer documents and never re-emit their own outputs, so
    /// peers that missed the original gossip (cache expiry, DHT miss, peer churn) can stall the
    /// node to its phase timeout.
    ///
    /// The artifacts are sent straight to libp2p as `GossipPublish`, bypassing both the EventBus
    /// dedup window (which already tracked them during replay) and the translator (which is only
    /// created on `EffectsEnabled`). Re-broadcasting the byte-identical original payload is
    /// equivocation-safe (peers dedup by event id) and idempotent. The query is bounded to the
    /// snapshot-cursor window so only the in-flight (un-delivered) artifacts are re-sent.
    fn maybe_rebroadcast_own_artifacts(&mut self, ctx: &mut actix::Context<Self>) {
        if self.rebroadcast_started || !self.net_ready {
            return;
        }
        let Some(since) = self.rebroadcast_since.clone() else {
            return;
        };
        self.rebroadcast_started = true;

        let id = CorrelationId::new();
        self.rebroadcast_query_ids.insert(id);
        info!("NetSyncManager: querying own forwardable artifacts for post-restart re-broadcast");
        if let Err(e) = self.eventstore.try_send(
            EventStoreQueryBy::<TsAgg>::new(id, since, ctx.address().recipient())
                .with_filter(EventStoreFilter::Source(EventSource::Local)),
        ) {
            error!("Failed to query EventStore for re-broadcast: {e}");
            self.rebroadcast_query_ids.remove(&id);
            self.rebroadcast_started = false;
        }
    }

    /// Re-gossip the node's own forwardable artifacts returned by the re-broadcast query.
    fn handle_rebroadcast_response(&mut self, events: Vec<InterfoldEvent>) {
        let mut count = 0usize;
        for event in events {
            if !EventTranslationService::is_forwardable_event(&event) {
                continue;
            }
            let data: GossipData = match event.try_into() {
                Ok(data) => data,
                Err(e) => {
                    warn!("Failed to convert own artifact to gossip data: {e}");
                    continue;
                }
            };
            if let Err(e) = self.tx.try_send(NetCommand::GossipPublish {
                topic: self.topic.clone(),
                data,
                correlation_id: CorrelationId::new(),
            }) {
                warn!("Failed to re-broadcast own artifact (channel full or closed): {e}");
            } else {
                count += 1;
            }
        }
        info!("NetSyncManager: re-broadcast {count} own forwardable artifact(s) after restart");
    }

    /// Apply a readiness decision: publish `NetReady`, or schedule the fallback timeout.
    fn apply_readiness(&mut self, decision: ReadinessDecision, ctx: &mut actix::Context<Self>) {
        match decision {
            ReadinessDecision::PublishReady => {
                if let Err(e) = self.publish_net_ready() {
                    error!("Failed to publish NetReady: {e}");
                }
                self.net_ready = true;
                self.maybe_rebroadcast_own_artifacts(ctx);
            }
            ReadinessDecision::WaitForConnection => {
                info!(
                    "All peer dials failed, waiting for connections before publishing NetReady..."
                );
                ctx.run_later(NET_READY_CONNECT_TIMEOUT, move |this, ctx| {
                    if let ReadinessDecision::PublishReady = this.readiness.on_connect_timeout() {
                        warn!("No peer connections established within 60s timeout, publishing NetReady anyway");
                        if let Err(e) = this.publish_net_ready() {
                            error!("Failed to publish NetReady: {e}");
                        }
                        this.net_ready = true;
                        this.maybe_rebroadcast_own_artifacts(ctx);
                    }
                });
            }
            ReadinessDecision::Idle => {}
        }
    }

    pub fn setup(
        bus: &BusHandle,
        tx: &mpsc::Sender<NetCommand>,
        rx: &Arc<broadcast::Receiver<NetEvent>>,
        eventstore: Recipient<EventStoreQueryBy<TsAgg>>,
        topic: &str,
    ) -> Addr<Self> {
        let mut events = rx.resubscribe();
        let addr = Self::new(bus, tx, rx, eventstore, topic).start();

        bus.subscribe(EventType::HistoricalNetSyncStart, addr.clone().recipient());

        // Forward from NetEvent
        tokio::spawn({
            debug!("Spawning event receive loop!");
            let addr = addr.clone();
            async move {
                while let Some(event) = super::recv_net_event(&mut events, "NetSyncManager").await {
                    debug!("Received event {:?}", event);
                    match event {
                        // Someone is asking for our sync
                        NetEvent::IncomingRequest(value) => addr.do_send(value),
                        NetEvent::AllPeersDialed { connected, total } => {
                            addr.do_send(AllPeersDialed { connected, total })
                        }
                        NetEvent::ConnectionEstablished { .. } => addr.do_send(PeerConnected),
                        _ => (),
                    }
                }
            }
        });

        addr
    }
}

impl Actor for NetSyncManager {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

/// Event broadcast from event bus
impl Handler<InterfoldEvent> for NetSyncManager {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        let (msg, ec) = msg.into_components();
        // We are making a sync request of another node
        if let InterfoldEventData::HistoricalNetSyncStart(data) = msg {
            // Capture the snapshot-cursor map so we can bound the post-restart re-broadcast of our
            // own forwardable artifacts to the in-flight window (H3/H11).
            self.rebroadcast_since = Some(data.since.clone().into_iter().collect());
            self.maybe_rebroadcast_own_artifacts(ctx);
            ctx.notify(TypedEvent::new(data, ec))
        }
    }
}

/// SyncRequest is called on start up to fetch remote events
impl Handler<TypedEvent<HistoricalNetSyncStart>> for NetSyncManager {
    type Result = ResponseFuture<()>;
    fn handle(
        &mut self,
        msg: TypedEvent<HistoricalNetSyncStart>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        info!("HISTORICAL_NET_SYNC_START");
        trap_fut(
            EType::Net,
            &self.bus.with_ec(msg.get_ctx()),
            handle_sync_request_event(
                self.tx.clone(),
                self.rx.clone(),
                msg,
                ctx.address(),
                !self.readiness_all_peers_dialed(),
            ),
        )
    }
}

impl NetSyncManager {
    fn readiness_all_peers_dialed(&self) -> bool {
        // `handle_sync_request_event` waits for a connection only if we have not yet observed the
        // AllPeersDialed signal. The readiness machine tracks this; mirror its view here.
        self.readiness.all_peers_dialed()
    }
}

/// We have received the sync response from the remote peer
impl Handler<TypedEvent<SyncRequestSucceeded>> for NetSyncManager {
    type Result = ();
    fn handle(
        &mut self,
        msg: TypedEvent<SyncRequestSucceeded>,
        _: &mut Self::Context,
    ) -> Self::Result {
        trap(EType::Net, &self.bus.with_ec(msg.get_ctx()), || {
            info!("SYNC REQUEST SUCCEEDED");
            let (msg, ctx) = msg.into_components();
            let response = msg.response;
            self.bus.publish_from_remote_as_response(
                HistoricalNetSyncEventsReceived {
                    events: response.events.to_vec(),
                },
                response.ts,
                ctx,
                None,
                EventSource::Net,
            )?;

            Ok(())
        });
    }
}

/// We have received a sync request from a remote peer
impl Handler<IncomingRequest> for NetSyncManager {
    type Result = ();
    fn handle(&mut self, msg: IncomingRequest, ctx: &mut Self::Context) -> Self::Result {
        trap(EType::Net, &self.bus.clone(), || {
            let IncomingRequest { peer, responder } = msg;
            let fetch_request: FetchEventsSince = match responder.try_request_into() {
                Ok(request) => request,
                Err(error) => {
                    warn!(%peer, %error, "Rejecting malformed historical-sync request");
                    responder.bad_request("malformed historical sync request")?;
                    return Ok(());
                }
            };
            if fetch_request.limit() == 0 {
                responder.bad_request("limit must be greater than 0")?;
                return Ok(());
            }
            if let Some(reason) = self.request_capacity_error(&peer) {
                warn!(
                    %peer,
                    in_flight = self.requests.len(),
                    "Rejecting historical-sync request: {reason}"
                );
                responder.bad_request(reason)?;
                return Ok(());
            }

            let id = CorrelationId::new();
            let scan_limit = sync_scan_limit(fetch_request.limit());
            info!(
                peer = %peer,
                correlation_id = %id,
                requested_limit = fetch_request.limit(),
                scan_limit,
                "Processing incoming historical-sync request"
            );
            let query: HashMap<AggregateId, u128> =
                HashMap::from([(fetch_request.aggregate_id(), fetch_request.since())]);
            self.requests
                .insert(id, PendingSyncRequest { peer, responder });
            let storage_query =
                EventStoreQueryBy::<TsAgg>::new(id, query, ctx.address().recipient())
                    .with_limit(scan_limit as u64);
            if let Err(error) = self.eventstore.try_send(storage_query) {
                if let Some(pending) = self.requests.remove(&id) {
                    pending.responder.respond(ProtocolResponse::Error(
                        "historical sync storage unavailable".to_string(),
                    ))?;
                }
                warn!(%peer, correlation_id = %id, %error, "Failed to query EventStore for sync");
                return Ok(());
            }
            ctx.run_later(INCOMING_SYNC_REQUEST_TIMEOUT, move |this, _| {
                this.expire_sync_request(id);
            });
            Ok(())
        });
    }
}

/// Receive Events from EventStore
impl Handler<EventStoreQueryResponse> for NetSyncManager {
    type Result = ();
    fn handle(&mut self, msg: EventStoreQueryResponse, _: &mut Self::Context) -> Self::Result {
        // Post-restart re-broadcast response (own forwardable artifacts) — handled separately from
        // peer sync-request responses.
        if self.rebroadcast_query_ids.remove(&msg.id()) {
            self.handle_rebroadcast_response(msg.into_events());
            return;
        }
        trap(EType::Net, &self.bus.clone(), || {
            info!("Received response from eventstore.");
            let Some(pending) = self.requests.remove(&msg.id()) else {
                bail!("responder not found for {}", msg.id());
            };

            let fetch_request: FetchEventsSince = pending.responder.try_request_into()?;
            match build_sync_batch(msg.into_events(), &fetch_request) {
                SyncBatchOutcome::BadRequest(reason) => pending.responder.bad_request(reason)?,
                SyncBatchOutcome::Batch(batch) => pending.responder.ok(batch)?,
            }

            Ok(())
        })
    }
}

impl Handler<AllPeersDialed> for NetSyncManager {
    type Result = ();
    fn handle(&mut self, msg: AllPeersDialed, ctx: &mut Self::Context) -> Self::Result {
        info!(
            "NetSyncManager: AllPeersDialed (connected={}, total={})",
            msg.connected, msg.total
        );
        let decision = self.readiness.on_all_peers_dialed(msg.connected, msg.total);
        self.apply_readiness(decision, ctx);
    }
}

impl Handler<PeerConnected> for NetSyncManager {
    type Result = ();
    fn handle(&mut self, _: PeerConnected, ctx: &mut Self::Context) -> Self::Result {
        let decision = self.readiness.on_peer_connected();
        if let ReadinessDecision::PublishReady = decision {
            info!("NetSyncManager: first peer connected");
        }
        self.apply_readiness(decision, ctx);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct AllPeersDialed {
    connected: usize,
    total: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
struct PeerConnected;

async fn fetch_historical_events_for_aggregate(
    net_cmds: &mpsc::Sender<NetCommand>,
    net_events: &Arc<broadcast::Receiver<NetEvent>>,
    aggregate_id: AggregateId,
    since: u128,
    budget: &mut SyncFetchBudget,
) -> Result<Vec<InterfoldEvent<Unsequenced>>> {
    let requester = DirectRequester::builder(net_cmds.clone(), net_events.clone())
        .max_retries(SYNC_FETCH_MAX_RETRIES)
        .retry_timeout(SYNC_FETCH_RETRY_TIMEOUT)
        .build();

    fetch_all_batched_events_with_budget::<InterfoldEvent<Unsequenced>>(
        requester,
        PeerTarget::Random,
        aggregate_id,
        since,
        100,
        budget,
    )
    .await
}

async fn handle_sync_request_event(
    net_cmds: mpsc::Sender<NetCommand>,
    net_events: Arc<broadcast::Receiver<NetEvent>>,
    event: TypedEvent<HistoricalNetSyncStart>,
    address: impl Into<Recipient<TypedEvent<SyncRequestSucceeded>>>,
    wait_for_event: bool,
) -> Result<()> {
    info!("Sync request event received");
    let (event, ctx) = event.into_components();
    info!("Checking for AllPeersDialed...");
    if wait_for_event {
        info!("Waiting for peer connection...");
        let has_peers = await_event(
            &net_events,
            |e| match e {
                NetEvent::ConnectionEstablished { .. } => {
                    info!("Peer connection established");
                    Some(true)
                }
                NetEvent::AllPeersDialed { total: 0, .. } => {
                    info!("No peers configured, proceeding without sync");
                    Some(false)
                }
                _ => None,
            },
            NET_READY_CONNECT_TIMEOUT,
        )
        .await
        .context("No peer connections established within timeout")?;

        if !has_peers {
            let value = SyncRequestSucceeded {
                response: SyncResponseValue {
                    events: vec![],
                    ts: 0,
                },
            };

            address.into().try_send(TypedEvent::new(value, ctx))?;
            return Ok(());
        }
    }
    info!("handle_sync_request_event: ready to sync");

    let mut all_events: Vec<InterfoldEvent<Unsequenced>> = Vec::new();
    let mut latest_timestamp: u128 = 0;
    let mut failed_aggregates: Vec<AggregateId> = Vec::new();
    let mut budget = SyncFetchBudget::production();

    for (aggregate_id, since) in event.since.iter() {
        info!(
            "Requesting batched events for aggregate_id={} since={}",
            aggregate_id, since
        );
        match fetch_historical_events_for_aggregate(
            &net_cmds,
            &net_events,
            *aggregate_id,
            *since,
            &mut budget,
        )
        .await
        {
            Ok(events) => {
                info!(
                    "Received {} events for aggregate_id={}",
                    events.len(),
                    aggregate_id
                );
                for interfold_event in events {
                    let ts = interfold_event.ts();
                    if ts > latest_timestamp {
                        latest_timestamp = ts;
                    }
                    all_events.push(interfold_event);
                }
            }
            Err(e) => {
                if budget.is_exhausted() {
                    return Err(e).context("historical net sync exhausted its global budget");
                }
                warn!(
                    "Failed to fetch events for aggregate_id={}: {e}. Continuing with available events.",
                    aggregate_id
                );
                failed_aggregates.push(*aggregate_id);
            }
        }
    }

    // If any aggregate failed, retry a few recovery rounds. Prefer a fresh
    // ConnectionEstablished signal when one arrives, but do not depend on it:
    // a connected peer may simply be slow or temporarily stalled.
    if !failed_aggregates.is_empty() {
        info!(
            "Sync fetch failed for {} aggregates — starting recovery retries...",
            failed_aggregates.len()
        );
        let mut recovery_attempt = 0;

        while !failed_aggregates.is_empty() && recovery_attempt < SYNC_RECOVERY_MAX_ATTEMPTS {
            recovery_attempt += 1;

            match await_event(
                &net_events,
                |e| {
                    if matches!(e, NetEvent::ConnectionEstablished { .. }) {
                        Some(())
                    } else {
                        None
                    }
                },
                SYNC_RECOVERY_RETRY_INTERVAL,
            )
            .await
            {
                Ok(()) => {
                    info!(
                        attempt = recovery_attempt,
                        "Peer reconnected, retrying failed aggregates"
                    );
                }
                Err(_) => {
                    info!(
                        attempt = recovery_attempt,
                        retry_after = ?SYNC_RECOVERY_RETRY_INTERVAL,
                        "No new peer connection observed; retrying failed aggregates against current peers"
                    );
                }
            }

            let mut still_failed = Vec::new();
            for aggregate_id in failed_aggregates {
                let since = event.since.get(&aggregate_id).copied().unwrap_or(0);
                match fetch_historical_events_for_aggregate(
                    &net_cmds,
                    &net_events,
                    aggregate_id,
                    since,
                    &mut budget,
                )
                .await
                {
                    Ok(events) => {
                        info!(
                            attempt = recovery_attempt,
                            "Retry succeeded: {} events for aggregate_id={}",
                            events.len(),
                            aggregate_id
                        );
                        for interfold_event in events {
                            let ts = interfold_event.ts();
                            if ts > latest_timestamp {
                                latest_timestamp = ts;
                            }
                            all_events.push(interfold_event);
                        }
                    }
                    Err(e) => {
                        if budget.is_exhausted() {
                            return Err(e)
                                .context("historical net sync exhausted its global budget");
                        }
                        warn!(
                            attempt = recovery_attempt,
                            "Retry failed for aggregate_id={}: {e}", aggregate_id
                        );
                        still_failed.push(aggregate_id);
                    }
                }
            }

            failed_aggregates = still_failed;
        }

        if !failed_aggregates.is_empty() {
            bail!(
                "failed to fetch historical net events for aggregates: {:?} after {} recovery attempts",
                failed_aggregates,
                SYNC_RECOVERY_MAX_ATTEMPTS
            );
        }
    }

    info!(
        "Sync complete: collected {} events across {} aggregates, latest_timestamp={}",
        all_events.len(),
        event.since.len(),
        latest_timestamp
    );

    let value = SyncRequestSucceeded {
        response: SyncResponseValue {
            events: all_events,
            ts: latest_timestamp,
        },
    };

    address.into().try_send(TypedEvent::new(value, ctx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        direct_responder::ChannelType,
        events::{IncomingRequest, NetCommand},
    };
    use actix::{Actor, Context as ActixContext, Handler};
    use e3_ciphernode_builder::EventSystem;
    use e3_events::{
        E3id, EventSource, InterfoldEvent, PlaintextAggregated, TestEvent, Unsequenced,
    };
    use e3_utils::ArcBytes;
    use tokio::sync::{broadcast, mpsc, mpsc::UnboundedSender};

    /// Minimal EventStore stand-in so `NetSyncManager::new` can be constructed in tests; the
    /// re-broadcast unit test drives `handle_rebroadcast_response` directly and never queries it.
    struct NoopEventStore;
    impl Actor for NoopEventStore {
        type Context = ActixContext<Self>;
    }
    impl Handler<EventStoreQueryBy<TsAgg>> for NoopEventStore {
        type Result = ();
        fn handle(&mut self, _: EventStoreQueryBy<TsAgg>, _: &mut Self::Context) {}
    }

    struct RecordingEventStore {
        queries: UnboundedSender<Option<u64>>,
    }

    impl Actor for RecordingEventStore {
        type Context = ActixContext<Self>;
    }

    impl Handler<EventStoreQueryBy<TsAgg>> for RecordingEventStore {
        type Result = ();
        fn handle(&mut self, msg: EventStoreQueryBy<TsAgg>, _: &mut Self::Context) {
            let _ = self.queries.send(msg.limit());
            // Intentionally retain no response. Tests exercise the manager's in-flight bounds.
        }
    }

    fn manager_with_recording_store(
        query_tx: UnboundedSender<Option<u64>>,
    ) -> (NetSyncManager, mpsc::Receiver<NetCommand>) {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle().unwrap().enable("test");
        let (tx, rx) = mpsc::channel::<NetCommand>(100);
        let (_evt_tx, evt_rx) = broadcast::channel::<NetEvent>(100);
        let evt_rx = Arc::new(evt_rx);
        let eventstore = RecordingEventStore { queries: query_tx }
            .start()
            .recipient();

        (
            NetSyncManager::new(&bus, &tx, &evt_rx, eventstore, "my-topic"),
            rx,
        )
    }

    fn incoming_sync_request(
        peer: PeerId,
        id: u64,
        limit: usize,
        tx: &mpsc::Sender<NetCommand>,
    ) -> IncomingRequest {
        let request: Vec<u8> = FetchEventsSince::new(AggregateId::new(1), 0, limit)
            .try_into()
            .unwrap();
        let responder = DirectResponder::new(id, ChannelType::Test(format!("request-{id}")), tx)
            .with_request(request);
        IncomingRequest { peer, responder }
    }

    fn protocol_response(command: NetCommand) -> ProtocolResponse {
        let NetCommand::IncomingResponse(incoming) = command else {
            panic!("expected IncomingResponse, got {command:?}");
        };
        incoming.responder.to_response().unwrap().1
    }

    fn local_forwardable_event(e3: &str) -> InterfoldEvent {
        InterfoldEvent::<Unsequenced>::new_with_timestamp(
            PlaintextAggregated {
                e3_id: E3id::new(e3, 1),
                decrypted_output: vec![ArcBytes::from_bytes(&[1, 2, 3, 4])],
                decryption_aggregator_proofs: vec![],
            }
            .into(),
            None,
            10,
            None,
            EventSource::Local,
        )
        .into_sequenced(1)
    }

    fn local_non_forwardable_event() -> InterfoldEvent {
        InterfoldEvent::<Unsequenced>::new_with_timestamp(
            TestEvent::new("not-forwardable", 1).into(),
            None,
            11,
            None,
            EventSource::Local,
        )
        .into_sequenced(2)
    }

    #[actix::test]
    async fn rebroadcast_only_gossips_forwardable_own_artifacts() {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle().unwrap().enable("test");
        let (tx, mut rx) = mpsc::channel::<NetCommand>(100);
        let (_evt_tx, evt_rx) = broadcast::channel::<NetEvent>(100);
        let evt_rx = Arc::new(evt_rx);
        let eventstore = NoopEventStore.start().recipient();

        let mut mgr = NetSyncManager::new(&bus, &tx, &evt_rx, eventstore, "my-topic");

        mgr.handle_rebroadcast_response(vec![
            local_forwardable_event("1234"),
            local_non_forwardable_event(),
        ]);

        // Exactly one GossipPublish for the forwardable artifact, on the configured topic.
        let cmd = rx.try_recv().expect("expected a GossipPublish command");
        let NetCommand::GossipPublish { topic, data, .. } = cmd else {
            panic!("expected GossipPublish, got {cmd:?}");
        };
        assert_eq!(topic, "my-topic");
        let event: InterfoldEvent<Unsequenced> = data.try_into().unwrap();
        assert!(matches!(
            event.get_data(),
            InterfoldEventData::PlaintextAggregated(_)
        ));

        // The non-forwardable event must not have produced a second command.
        assert!(
            rx.try_recv().is_err(),
            "non-forwardable event should not be re-broadcast"
        );
    }

    #[actix::test]
    async fn malicious_huge_limit_is_capped_before_storage_query() {
        let (query_tx, mut query_rx) = mpsc::unbounded_channel();
        let (manager, _net_rx) = manager_with_recording_store(query_tx);
        let net_tx = manager.tx.clone();
        let manager = manager.start();

        manager
            .send(incoming_sync_request(
                PeerId::random(),
                1,
                usize::MAX,
                &net_tx,
            ))
            .await
            .unwrap();

        let queried_limit = tokio::time::timeout(Duration::from_secs(1), query_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(queried_limit, Some(sync_scan_limit(usize::MAX) as u64));
    }

    #[actix::test]
    async fn concurrent_sync_requests_are_globally_bounded() {
        let (query_tx, mut query_rx) = mpsc::unbounded_channel();
        let (manager, mut net_rx) = manager_with_recording_store(query_tx);
        let net_tx = manager.tx.clone();
        let manager = manager.start();

        for id in 0..MAX_IN_FLIGHT_SYNC_REQUESTS as u64 {
            manager
                .send(incoming_sync_request(PeerId::random(), id, 1, &net_tx))
                .await
                .unwrap();
        }
        manager
            .send(incoming_sync_request(
                PeerId::random(),
                MAX_IN_FLIGHT_SYNC_REQUESTS as u64,
                1,
                &net_tx,
            ))
            .await
            .unwrap();

        for _ in 0..MAX_IN_FLIGHT_SYNC_REQUESTS {
            tokio::time::timeout(Duration::from_secs(1), query_rx.recv())
                .await
                .unwrap()
                .unwrap();
        }
        assert!(
            query_rx.try_recv().is_err(),
            "overflow request reached storage"
        );
        let response = tokio::time::timeout(Duration::from_secs(1), net_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            protocol_response(response),
            ProtocolResponse::BadRequest(reason) if reason.contains("too many in-flight")
        ));
    }

    #[actix::test]
    async fn concurrent_sync_requests_are_bounded_per_authenticated_peer() {
        let (query_tx, mut query_rx) = mpsc::unbounded_channel();
        let (manager, mut net_rx) = manager_with_recording_store(query_tx);
        let net_tx = manager.tx.clone();
        let manager = manager.start();
        let peer = PeerId::random();

        for id in 0..=MAX_IN_FLIGHT_SYNC_REQUESTS_PER_PEER as u64 {
            manager
                .send(incoming_sync_request(peer, id, 1, &net_tx))
                .await
                .unwrap();
        }

        for _ in 0..MAX_IN_FLIGHT_SYNC_REQUESTS_PER_PEER {
            tokio::time::timeout(Duration::from_secs(1), query_rx.recv())
                .await
                .unwrap()
                .unwrap();
        }
        assert!(
            query_rx.try_recv().is_err(),
            "per-peer overflow reached storage"
        );
        let response = tokio::time::timeout(Duration::from_secs(1), net_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            protocol_response(response),
            ProtocolResponse::BadRequest(reason) if reason.contains("this peer")
        ));
    }

    #[actix::test]
    async fn timed_out_sync_request_releases_its_in_flight_slot() {
        let (query_tx, _query_rx) = mpsc::unbounded_channel();
        let (mut manager, mut net_rx) = manager_with_recording_store(query_tx);
        let net_tx = manager.tx.clone();
        let peer = PeerId::random();
        let IncomingRequest { responder, .. } = incoming_sync_request(peer, 1, 1, &net_tx);
        let id = CorrelationId::new();
        manager
            .requests
            .insert(id, PendingSyncRequest { peer, responder });

        manager.expire_sync_request(id);

        assert!(manager.requests.is_empty());
        let response = tokio::time::timeout(Duration::from_secs(1), net_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            protocol_response(response),
            ProtocolResponse::Error(reason) if reason.contains("timed out")
        ));
    }
}
