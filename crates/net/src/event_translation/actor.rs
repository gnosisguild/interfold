// SPDX-License-Identifier: LGPL-3.0-only
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::domain::EventTranslationService;
use crate::events::{GossipData, NetCommand, NetEvent};
use crate::NetworkPolicy;
use actix::prelude::*;
use anyhow::Result;
use e3_events::{
    prelude::*, trap, BusHandle, CorrelationId, EType, EventContextAccessors, EventId, EventSource,
    EventType, InterfoldEvent,
};
use e3_utils::MAILBOX_LIMIT;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// NetEventTranslator Actor converts between EventBus events and Libp2p events forwarding them to a
/// Libp2pNetInterface for propagation over the p2p network. All translation/dedup decisions live
/// in [`EventTranslationService`].
pub struct NetEventTranslator {
    bus: BusHandle,
    tx: mpsc::Sender<NetCommand>,
    service: EventTranslationService,
    pending: HashMap<CorrelationId, PendingPublish>,
}

const MAX_GOSSIP_PUBLISH_ATTEMPTS: u8 = 3;
const GOSSIP_RETRY_DELAY: Duration = Duration::from_secs(2);

struct PendingPublish {
    event_id: EventId,
    data: GossipData,
    attempt: u8,
}

impl Actor for NetEventTranslator {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
    }
}

/// Libp2pEvent is used to send data to the Libp2pNetInterface from the NetEventTranslator
#[derive(Message, Clone, Debug, PartialEq, Eq)]
#[rtype(result = "()")]
struct LibP2pEvent(pub GossipData);

impl NetEventTranslator {
    /// Create a new NetEventTranslator actor
    pub fn new(
        bus: &BusHandle,
        tx: &mpsc::Sender<NetCommand>,
        topic: &str,
        network: NetworkPolicy,
    ) -> Self {
        Self {
            bus: bus.clone(),
            tx: tx.clone(),
            service: EventTranslationService::with_network(topic, network),
            pending: HashMap::new(),
        }
    }

    pub fn setup(
        bus: &BusHandle,
        tx: &mpsc::Sender<NetCommand>,
        rx: &Arc<broadcast::Receiver<NetEvent>>,
        topic: &str,
        network: NetworkPolicy,
    ) -> Addr<Self> {
        let mut rx = rx.resubscribe();
        let addr = NetEventTranslator::new(bus, tx, topic, network).start();

        // Listen on all events
        bus.subscribe(EventType::All, addr.clone().recipient());
        info!("NetEventTranslator is running");
        tokio::spawn({
            let addr = addr.clone();
            async move {
                while let Some(event) =
                    crate::event_subscription::recv_net_event(&mut rx, "NetEventTranslator").await
                {
                    let delivery = match event {
                        NetEvent::GossipData(data @ GossipData::GossipBytes(_)) => {
                            addr.send(TranslatorMessage::Inbound(data)).await
                        }
                        NetEvent::GossipPublished { correlation_id, .. } => {
                            addr.send(TranslatorMessage::PublishSucceeded(correlation_id))
                                .await
                        }
                        NetEvent::GossipPublishError {
                            correlation_id,
                            error,
                        } => {
                            addr.send(TranslatorMessage::PublishFailed {
                                correlation_id,
                                reason: error.to_string(),
                            })
                            .await
                        }
                        _ => continue,
                    };
                    if let Err(error) = delivery {
                        warn!(%error, "NetEventTranslator stopped; ending gossip ingress");
                        break;
                    }
                }
            }
        });

        addr
    }

    /// Function to determine which events are allowed to be automatically broadcast to the
    /// network. Kept here so the rule can be referenced via `NetEventTranslator` while the
    /// implementation lives in the pure service.
    pub fn is_forwardable_event(event: &InterfoldEvent) -> bool {
        EventTranslationService::is_forwardable_event(event)
    }

    fn handle_interfold_event(
        &mut self,
        msg: InterfoldEvent,
        ctx: &mut Context<Self>,
    ) -> Result<()> {
        if let Some((event_id, data)) = self.service.prepare_outbound(msg)? {
            self.queue_publish(event_id, data, 1, ctx);
        }
        Ok(())
    }

    fn queue_publish(
        &mut self,
        event_id: EventId,
        data: GossipData,
        attempt: u8,
        ctx: &mut Context<Self>,
    ) {
        let correlation_id = CorrelationId::new();
        let command = NetCommand::GossipPublish {
            topic: self.service.topic().to_owned(),
            data: data.clone(),
            correlation_id,
        };
        self.pending.insert(
            correlation_id,
            PendingPublish {
                event_id,
                data,
                attempt,
            },
        );
        let tx = self.tx.clone();
        ctx.spawn(async move { tx.send(command).await }.into_actor(self).map(
            move |result, actor, ctx| {
                if let Err(error) = result {
                    actor.handle_publish_failed(
                        correlation_id,
                        format!("network command queue closed: {error}"),
                        ctx,
                    );
                }
            },
        ));
    }

    fn handle_publish_failed(
        &mut self,
        correlation_id: CorrelationId,
        reason: String,
        ctx: &mut Context<Self>,
    ) {
        let Some(pending) = self.pending.remove(&correlation_id) else {
            return;
        };
        if pending.attempt < MAX_GOSSIP_PUBLISH_ATTEMPTS {
            warn!(
                attempt = pending.attempt,
                %reason,
                "Gossip publish failed; scheduling retry"
            );
            ctx.run_later(GOSSIP_RETRY_DELAY, move |actor, ctx| {
                actor.queue_publish(pending.event_id, pending.data, pending.attempt + 1, ctx);
            });
        } else {
            self.service.mark_failed(pending.event_id);
            warn!(attempts = pending.attempt, %reason, "Gossip publish retries exhausted");
        }
    }

    fn handle_remote_event(&mut self, msg: LibP2pEvent) -> Result<()> {
        let event = self.service.prepare_inbound(msg.0)?;
        let (data, ec) = event.into_components();
        self.bus
            .publish_from_remote(data, ec.ts(), None, EventSource::Net)?;
        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "()")]
enum TranslatorMessage {
    Inbound(GossipData),
    PublishSucceeded(CorrelationId),
    PublishFailed {
        correlation_id: CorrelationId,
        reason: String,
    },
}

impl Handler<TranslatorMessage> for NetEventTranslator {
    type Result = ();
    fn handle(&mut self, msg: TranslatorMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            TranslatorMessage::Inbound(data) => {
                trap(EType::Net, &self.bus.clone(), || {
                    self.handle_remote_event(LibP2pEvent(data))
                });
            }
            TranslatorMessage::PublishSucceeded(correlation_id) => {
                if let Some(pending) = self.pending.remove(&correlation_id) {
                    self.service.mark_published(pending.event_id);
                }
            }
            TranslatorMessage::PublishFailed {
                correlation_id,
                reason,
            } => self.handle_publish_failed(correlation_id, reason, ctx),
        }
    }
}

impl Handler<InterfoldEvent> for NetEventTranslator {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        trap(EType::Net, &self.bus.with_ec(msg.get_ctx()), || {
            self.handle_interfold_event(msg, ctx)
        })
    }
}
