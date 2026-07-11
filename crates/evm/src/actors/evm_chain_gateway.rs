// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::domain::chain_sync_state::SyncStatus;
use crate::messages::HistoricalSyncComplete;
use crate::messages::InterfoldEvmEvent;
use actix::{Actor, ActorContext, Handler};
use actix::{Addr, Recipient};
use anyhow::{bail, Context, Result};
use e3_events::EType;
use e3_events::{
    BusHandle, ErrorDispatcher, EventSubscriber, EventType, HistoricalEvmEventsReceived,
    HistoricalEvmSyncStart, InterfoldEvent, InterfoldEventData, SyncEnded, Unsequenced,
};
use e3_events::{Event, EventPublisher};
use e3_utils::MAILBOX_LIMIT;
use tokio::sync::oneshot;
use tracing::warn;

/// Per-chain bound for events accumulated while the node is synchronizing.
///
/// Tests inject a smaller value. Production deliberately fails startup instead
/// of dropping an observed chain event if this window is exhausted.
pub const DEFAULT_MAX_BUFFERED_EVM_EVENTS: usize = 100_000;

pub struct EvmChainGatewayHandle {
    addr: Addr<EvmChainGateway>,
    readiness: oneshot::Receiver<std::result::Result<(), String>>,
}

impl EvmChainGatewayHandle {
    pub fn addr(&self) -> Addr<EvmChainGateway> {
        self.addr.clone()
    }

    pub async fn wait_until_live(self) -> Result<()> {
        self.readiness
            .await
            .context("EVM chain gateway stopped before reporting startup status")?
            .map_err(anyhow::Error::msg)
    }
}

/// This component sits between the Evm ingestion for a chain and the Sync actor and the Bus.
/// It coordinates event flow between these components.
pub struct EvmChainGateway {
    bus: BusHandle,
    status: SyncStatus<Recipient<HistoricalEvmEventsReceived>>,
    max_buffered_events: usize,
    readiness: Option<oneshot::Sender<std::result::Result<(), String>>>,
}

impl EvmChainGateway {
    pub fn new(bus: &BusHandle) -> Self {
        Self::with_options(bus, DEFAULT_MAX_BUFFERED_EVM_EVENTS, None)
    }

    fn with_options(
        bus: &BusHandle,
        max_buffered_events: usize,
        readiness: Option<oneshot::Sender<std::result::Result<(), String>>>,
    ) -> Self {
        Self {
            bus: bus.clone(),
            status: SyncStatus::default(),
            max_buffered_events,
            readiness,
        }
    }

    pub fn setup(bus: &BusHandle) -> Addr<Self> {
        Self::start_and_subscribe(bus, Self::new(bus))
    }

    pub fn setup_with_readiness(bus: &BusHandle) -> EvmChainGatewayHandle {
        Self::setup_with_readiness_and_limit(bus, DEFAULT_MAX_BUFFERED_EVM_EVENTS)
    }

    pub fn setup_with_readiness_and_limit(
        bus: &BusHandle,
        max_buffered_events: usize,
    ) -> EvmChainGatewayHandle {
        let (tx, readiness) = oneshot::channel();
        let actor = Self::with_options(bus, max_buffered_events, Some(tx));
        let addr = Self::start_and_subscribe(bus, actor);
        EvmChainGatewayHandle { addr, readiness }
    }

    fn start_and_subscribe(bus: &BusHandle, actor: Self) -> Addr<Self> {
        let addr = actor.start();
        bus.subscribe_all(
            &[EventType::HistoricalEvmSyncStart, EventType::SyncEnded],
            addr.clone().recipient(),
        );
        addr
    }

    fn signal_startup(&mut self, result: std::result::Result<(), String>) {
        if let Some(sender) = self.readiness.take() {
            let _ = sender.send(result);
        }
    }

    fn fail_closed(&mut self, error: anyhow::Error, ctx: &mut actix::Context<Self>) {
        let reason = format!(
            "EVM chain gateway failed closed: {error:#}. The gateway stopped and will not process \
             further chain events; inspect the snapshot/deploy block and RPC catch-up range, then \
             restart the node to replay chain history"
        );
        self.status.fail(reason.clone());
        self.signal_startup(Err(reason.clone()));
        self.bus.err(EType::Evm, anyhow::anyhow!(reason));
        ctx.stop();
    }

    fn handle_sync_start(&mut self, msg: HistoricalEvmSyncStart) -> Result<()> {
        let sender = msg
            .sender
            .context("No sender on HistoricalEvmSyncStart Message")?;
        let (mut buffer, pending_sync_complete) = self.status.forward_to_sync_actor(sender)?;

        for evt in buffer.drain(..) {
            self.process_evm_event(evt)?;
        }

        // HistoricalSyncComplete may have arrived before HistoricalEvmSyncStart
        if let Some(event) = pending_sync_complete {
            warn!("Processing buffered HistoricalSyncComplete that arrived during Init");
            self.forward_historical_sync_complete(event)?;
        }
        Ok(())
    }

    fn handle_sync_ended(&mut self, _: SyncEnded) -> Result<()> {
        let buffer = self.status.live()?;
        for evt in buffer {
            self.publish_evm_event(evt)?;
        }
        self.signal_startup(Ok(()));
        Ok(())
    }

    fn publish_evm_event(&mut self, msg: InterfoldEvent<Unsequenced>) -> Result<()> {
        self.bus.naked_dispatch(msg);
        Ok(())
    }

    fn handle_evm_event(&mut self, msg: InterfoldEvmEvent) -> Result<()> {
        match msg {
            InterfoldEvmEvent::HistoricalSyncComplete(e) => {
                self.forward_historical_sync_complete(e)?;
                Ok(())
            }
            InterfoldEvmEvent::Event(event) => {
                self.process_evm_event(event.into_interfold_event(&self.bus)?)?;
                Ok(())
            }
            InterfoldEvmEvent::Log(_) => {
                bail!("EvmChainGateway received an unparsed EVM log")
            }
            InterfoldEvmEvent::Rejected(rejected) => bail!(
                "chain {} rejected provider log {}: {}",
                rejected.chain_id,
                rejected.id,
                rejected.reason
            ),
            InterfoldEvmEvent::Processed(_) => {
                bail!("EvmChainGateway received an internal ordering marker")
            }
        }
    }

    fn forward_historical_sync_complete(&mut self, event: HistoricalSyncComplete) -> Result<()> {
        // Buffer if we're still in Init - will be replayed when HistoricalEvmSyncStart arrives
        if let SyncStatus::Init {
            pending_sync_complete,
            ..
        } = &mut self.status
        {
            warn!(
                chain_id = event.chain_id,
                "HistoricalSyncComplete arrived during Init, buffering"
            );
            *pending_sync_complete = Some(event);
            return Ok(());
        }

        let state = self.status.buffer_until_live()?;
        let sender = state
            .sender
            .context("ForwardToSyncActor state must hold a sender")?;
        let event = HistoricalEvmEventsReceived::new(state.buffer, event.chain_id);
        sender.try_send(event)?;
        Ok(())
    }

    fn process_evm_event(&mut self, msg: InterfoldEvent<Unsequenced>) -> Result<()> {
        if matches!(self.status, SyncStatus::Live) {
            return self.publish_evm_event(msg);
        }
        self.status
            .add_buffered_event(msg, self.max_buffered_events)
    }
}

impl Actor for EvmChainGateway {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        self.signal_startup(Err(
            "EVM chain gateway stopped before reaching Live; inspect preceding EVM errors"
                .to_owned(),
        ));
    }
}

impl Handler<InterfoldEvent> for EvmChainGateway {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        let result = (|| {
            match msg.into_data() {
                InterfoldEventData::HistoricalEvmSyncStart(e) => self.handle_sync_start(e)?,
                InterfoldEventData::SyncEnded(e) => self.handle_sync_ended(e)?,
                _ => (),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_closed(error, ctx);
        }
    }
}

impl Handler<InterfoldEvmEvent> for EvmChainGateway {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvmEvent, ctx: &mut Self::Context) -> Self::Result {
        if let Err(error) = self.handle_evm_event(msg) {
            self.fail_closed(error, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{EvmEvent, EvmLogRejected};

    use super::*;
    use e3_ciphernode_builder::EventSystem;

    use e3_events::{CorrelationId, EvmEventConfig, EvmEventConfigChain, TakeEvents, TestEvent};
    use tokio::sync::mpsc;
    use tracing_subscriber::{fmt, EnvFilter};

    struct SyncEventCollector {
        tx: mpsc::UnboundedSender<HistoricalEvmEventsReceived>,
    }

    #[actix::test]
    async fn rejected_log_fails_gateway_readiness() -> Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus = system.handle()?.enable("test-rejected-log");
        let gateway = EvmChainGateway::setup_with_readiness(&bus);

        gateway
            .addr()
            .send(InterfoldEvmEvent::Rejected(EvmLogRejected::new(
                CorrelationId::new(),
                1,
                "malformed historical log",
            )))
            .await?;

        let error = gateway.wait_until_live().await.unwrap_err();
        assert!(error.to_string().contains("malformed historical log"));
        Ok(())
    }

    impl Actor for SyncEventCollector {
        type Context = actix::Context<Self>;
    }

    impl Handler<HistoricalEvmEventsReceived> for SyncEventCollector {
        type Result = ();
        fn handle(&mut self, msg: HistoricalEvmEventsReceived, _: &mut Self::Context) {
            let _ = self.tx.send(msg);
        }
    }

    #[actix::test]
    async fn test_evm_chain_gateway() -> Result<()> {
        let _foo = tracing::subscriber::set_default(
            fmt()
                .with_env_filter(EnvFilter::new("info"))
                .with_test_writer()
                .finish(),
        );

        let system = EventSystem::new().with_fresh_bus();
        let bus: BusHandle = system.handle()?.enable("test");

        let history_collector = bus.history();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let collector = SyncEventCollector { tx }.start();

        let gateway = EvmChainGateway::setup_with_readiness(&bus);
        let addr = gateway.addr();

        let chain_id = 1u64;

        // HistoricalEvmSyncStart: Init -> ForwardToSyncActor
        let mut evm_config = EvmEventConfig::new();
        evm_config.insert(chain_id, EvmEventConfigChain::new(0));
        bus.publish_without_context(HistoricalEvmSyncStart::new(collector.clone(), evm_config))
            .unwrap();

        // Send EVM event while forwarding - should reach collector
        let evm_event = EvmEvent::new(
            CorrelationId::new(),
            TestEvent::new("Before Complete", 1).into(),
            100,
            12345,
            chain_id,
        );

        // This will actually arrive earlier than HistoricalEvmSyncStart but aught to be buffered
        addr.send(InterfoldEvmEvent::Event(evm_event)).await?;

        // HistoricalSyncComplete: ForwardToSyncActor -> BufferUntilLive
        addr.send(InterfoldEvmEvent::HistoricalSyncComplete(
            HistoricalSyncComplete::new(chain_id, None),
        ))
        .await?;

        // Normal Synchronizer will take this and wait for other events before flushing events to
        // the bus here we simulate it
        let received = rx.recv().await.unwrap();
        for event in received.events {
            bus.naked_dispatch(event);
        }

        // Send EVM event while buffering - should be buffered (not received)
        let buffered_event = EvmEvent::new(
            CorrelationId::new(),
            TestEvent::new("Before SyncEnded", 2).into(),
            101,
            12346,
            chain_id,
        );
        addr.send(InterfoldEvmEvent::Event(buffered_event)).await?;

        // The Synchronizer will publish the SyncEnded event when it has all the information it needs
        // and has published everything to the bus
        bus.publish_without_context(SyncEnded::new())?;
        gateway.wait_until_live().await?;

        let after_event = EvmEvent::new(
            CorrelationId::new(),
            TestEvent::new("After SyncEnded", 2).into(),
            101,
            12346,
            chain_id,
        );

        addr.send(InterfoldEvmEvent::Event(after_event)).await?;

        let full = history_collector.send(TakeEvents::new(5)).await?;

        let test_events: Vec<String> = full
            .events
            .iter()
            .filter_map(|e| {
                if let InterfoldEventData::TestEvent(TestEvent { msg, .. }) = e.get_data() {
                    Some(msg.to_string())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            test_events,
            vec!["Before Complete", "Before SyncEnded", "After SyncEnded"]
        );

        let event_types: Vec<String> = full.events.iter().map(|e| e.event_type()).collect();

        assert_eq!(
            event_types,
            vec![
                "HistoricalEvmSyncStart",
                "TestEvent",
                "SyncEnded",
                "TestEvent",
                "TestEvent"
            ]
        );
        Ok(())
    }

    #[actix::test]
    async fn overflow_emits_actionable_error_stops_and_fails_readiness() -> Result<()> {
        let system = EventSystem::new().with_fresh_bus();
        let bus: BusHandle = system.handle()?.enable("test-overflow");
        let errors = bus.errors();
        let gateway = EvmChainGateway::setup_with_readiness_and_limit(&bus, 1);
        let addr = gateway.addr();

        for entropy in [1, 2] {
            let event = EvmEvent::new(
                CorrelationId::new(),
                TestEvent::new("overflow", entropy).into(),
                100,
                u128::from(entropy),
                1,
            );
            addr.send(InterfoldEvmEvent::Event(event)).await?;
        }

        let startup_error = gateway
            .wait_until_live()
            .await
            .expect_err("overflow must fail gateway readiness")
            .to_string();
        assert!(startup_error.contains("Init buffer reached its limit of 1 events"));
        assert!(startup_error.contains("will not process further chain events"));
        assert!(startup_error.contains("restart the node to replay chain history"));

        let received = errors.send(TakeEvents::new(1)).await?;
        assert!(!received.timed_out, "overflow error should be observable");
        let InterfoldEventData::InterfoldError(error) = received.events[0].get_data() else {
            panic!("expected an InterfoldError event");
        };
        assert!(error
            .message
            .contains("Init buffer reached its limit of 1 events"));
        assert!(error.message.contains("snapshot/deploy block"));

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while addr.connected() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .context("overflowed gateway did not stop")?;
        assert!(!addr.connected(), "overflowed gateway must stop");
        Ok(())
    }
}
