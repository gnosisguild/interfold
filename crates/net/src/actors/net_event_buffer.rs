// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::{Actor, ActorContext, AsyncContext, Handler, Message};
use anyhow::{anyhow, Context, Result};
use e3_events::{
    BusHandle, EType, ErrorDispatcher, Event, EventSubscriber, EventType, InterfoldEvent,
    InterfoldEventData,
};
use e3_utils::MAILBOX_LIMIT;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::oneshot;

use crate::domain::net_buffer::{BufferDecision, NetEventBufferState};
use crate::events::NetEvent;

pub const DEFAULT_MAX_BUFFERED_NET_EVENTS: usize = 1_024;
pub const DEFAULT_MAX_BUFFERED_NET_BYTES: usize = 256 * 1024 * 1024;

pub struct NetEventBufferHandle {
    readiness: oneshot::Receiver<std::result::Result<(), String>>,
}

impl NetEventBufferHandle {
    pub async fn wait_until_running(self) -> Result<()> {
        self.readiness
            .await
            .context("network event buffer stopped before reporting startup status")?
            .map_err(anyhow::Error::msg)
    }
}

/// Actor that controls a broadcast channel which will buffer NetEvents until it receives a
/// `SyncEnded` event, at which time it releases all buffered events to the output channel. The
/// buffering decision logic lives in [`NetEventBufferState`].
pub struct NetEventBuffer {
    state: NetEventBufferState,
    input_rx: Option<broadcast::Receiver<NetEvent>>,
    output_tx: broadcast::Sender<NetEvent>,
    bus: BusHandle,
    max_events: usize,
    max_bytes: usize,
    readiness: Option<oneshot::Sender<std::result::Result<(), String>>>,
}

impl NetEventBuffer {
    pub(crate) fn setup_with_limits(
        bus: &BusHandle,
        input_rx: &broadcast::Receiver<NetEvent>,
        max_events: usize,
        max_bytes: usize,
    ) -> (broadcast::Receiver<NetEvent>, NetEventBufferHandle) {
        let input_rx = input_rx.resubscribe();
        let (output_tx, output_rx) = broadcast::channel(max_events);
        let (readiness_tx, readiness) = oneshot::channel();

        let actor = Self {
            state: NetEventBufferState::syncing(),
            input_rx: Some(input_rx),
            output_tx,
            bus: bus.clone(),
            max_events,
            max_bytes,
            readiness: Some(readiness_tx),
        };

        let addr = actor.start();

        // Subscribe to InterfoldEvent on the bus
        bus.subscribe(EventType::SyncEnded, addr.clone().recipient());

        (output_rx, NetEventBufferHandle { readiness })
    }

    fn handle_interfold_event(&mut self, msg: InterfoldEvent) -> Result<()> {
        if let InterfoldEventData::SyncEnded(_) = msg.get_data() {
            return self.process_sync_ended();
        }
        Ok(())
    }

    fn process_sync_ended(&mut self) -> Result<()> {
        let pending = self.state.run()?;
        for event in pending {
            self.forward_event(event)?;
        }
        self.signal_startup(Ok(()));
        Ok(())
    }

    fn forward_event(&mut self, event: NetEvent) -> Result<()> {
        self.output_tx
            .send(event)
            .map_err(|e| anyhow!("Failed to forward event: {}", e))?;
        Ok(())
    }

    fn signal_startup(&mut self, result: std::result::Result<(), String>) {
        if let Some(sender) = self.readiness.take() {
            let _ = sender.send(result);
        }
    }

    fn fail_closed(&mut self, error: anyhow::Error, ctx: &mut actix::Context<Self>) {
        let reason = format!(
            "network event buffer failed closed: {error:#}; startup will stop rather than drop \
             live protocol input. Increase the configured buffer only after measuring the sync \
             backlog, or restore peer/RPC health and restart"
        );
        self.signal_startup(Err(reason.clone()));
        self.bus.err(EType::Net, anyhow!(reason));
        ctx.stop();
    }
}

impl Actor for NetEventBuffer {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
        // Spawn task to read from broadcast channel
        let addr = ctx.address();
        let mut input_rx = self.input_rx.take().expect("input_rx should be present");

        actix::spawn(async move {
            loop {
                match input_rx.recv().await {
                    Ok(event) => {
                        if addr.send(IncomingNetEvent(event)).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        let _ = addr.send(NetInputLagged(skipped)).await;
                        break;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        self.signal_startup(Err(
            "network event buffer stopped before startup synchronization completed".to_owned(),
        ));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct IncomingNetEvent(NetEvent);

#[derive(Message)]
#[rtype(result = "()")]
struct NetInputLagged(u64);

impl Handler<IncomingNetEvent> for NetEventBuffer {
    type Result = ();

    fn handle(&mut self, msg: IncomingNetEvent, ctx: &mut Self::Context) {
        let event_bytes = if self.state.is_running() {
            0
        } else {
            msg.0.buffered_size_bytes()
        };
        let result = self
            .state
            .observe(msg.0, event_bytes, self.max_events, self.max_bytes)
            .and_then(|decision| match decision {
                BufferDecision::Buffered => Ok(()),
                BufferDecision::Forward(event) => self.forward_event(event),
            });
        if let Err(error) = result {
            self.fail_closed(error, ctx);
        }
    }
}

impl Handler<NetInputLagged> for NetEventBuffer {
    type Result = ();

    fn handle(&mut self, msg: NetInputLagged, ctx: &mut Self::Context) {
        self.fail_closed(
            anyhow!(
                "network event input skipped {} events because its bounded broadcast receiver lagged",
                msg.0
            ),
            ctx,
        );
    }
}

impl Handler<InterfoldEvent> for NetEventBuffer {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        if let Err(error) = self.handle_interfold_event(msg) {
            self.fail_closed(error, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
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

        let received3 =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), output_rx.recv())
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
}
