// SPDX-License-Identifier: LGPL-3.0-only

//! Mailbox entry points and lifecycle hooks.

use super::*;
use e3_events::{EventContext, RequestRouterCheckpoint, Sequenced};

impl E3Router {
    fn checkpoint_with_context(&mut self, context: &EventContext<Sequenced>) -> Result<()> {
        self.replay_cursors
            .insert(context.aggregate_id(), context.seq());
        let snapshot = self.snapshot()?;
        self.recovery_store.write_with_context(
            &RequestRouterCheckpoint {
                contexts: snapshot.contexts,
                completed: snapshot.completed,
                replay_cursors: self.replay_cursors.clone(),
            },
            context,
        )?;
        Ok(())
    }
}

impl Actor for E3Router {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

impl Handler<InterfoldEvent> for E3Router {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, _: &mut Self::Context) -> Self::Result {
        trap(EType::Event, &self.bus.with_ec(msg.get_ctx()), || {
            let event_context = msg.get_ctx().clone();
            let result = match RequestRouter::route_with_context(
                &msg,
                &self.completed,
                msg.get_e3_id()
                    .is_some_and(|e3_id| self.contexts.contains_key(&e3_id)),
            ) {
                RoutingDecision::Broadcast => {
                    for context in self.contexts.values() {
                        context.forward_message_now(&msg)
                    }
                    Ok(())
                }
                RoutingDecision::Ignore => Ok(()),
                RoutingDecision::AlreadyCompleted(e3_id) => Err(anyhow!(
                    "unexpected {} for completed E3 {} (event={}, origin={}, source={:?}, block={:?})",
                    msg.event_type(),
                    e3_id,
                    msg.id(),
                    msg.origin_id(),
                    msg.source(),
                    msg.block(),
                )),
                RoutingDecision::UnadmittedNetworkEvent(e3_id) => Err(anyhow!(
                    "rejected {} for unknown E3 {} from the peer network (event={}, origin={})",
                    msg.event_type(),
                    e3_id,
                    msg.id(),
                    msg.origin_id(),
                )),
                RoutingDecision::Process {
                    e3_id,
                    post_forward,
                } => {
                    let repositories = self.store.repositories();
                    let context = self.contexts.entry(e3_id.clone()).or_insert_with(|| {
                        E3Context::from_params(E3ContextParams {
                            e3_id: e3_id.clone(),
                            repository: repositories.context(&e3_id),
                            extensions: self.extensions.clone(),
                        })
                    });

                    for extension in self.extensions.iter() {
                        extension.on_event(context, &msg);
                    }

                    context.forward_message(&msg, &mut self.buffer);
                    if post_forward != PostForward::Teardown {
                        context
                            .repository
                            .write_with_context(&context.snapshot()?, &event_context)?;
                    }

                    let (_, ctx) = msg.into_components();
                    match post_forward {
                        PostForward::PublishComplete => {
                            self.bus.publish(
                                E3RequestComplete {
                                    e3_id: e3_id.clone(),
                                },
                                ctx,
                            )?;
                        }
                        PostForward::Teardown => {
                            self.contexts.remove(&e3_id);
                            self.buffer.remove_e3(&e3_id);
                            self.completed.insert(e3_id);
                        }
                        PostForward::None => (),
                    }

                    Ok(())
                }
            };

            let checkpoint_result = self.checkpoint_with_context(&event_context);
            result.and(checkpoint_result)
        });
    }
}
