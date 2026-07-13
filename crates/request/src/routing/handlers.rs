// SPDX-License-Identifier: LGPL-3.0-only

//! Mailbox entry points and lifecycle hooks.

use super::*;

impl Actor for E3Router {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

impl Handler<InterfoldEvent> for E3Router {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, _: &mut Self::Context) -> Self::Result {
        trap(
            EType::Event,
            &self.bus.with_ec(msg.get_ctx()),
            || {
                match RequestRouter::route(&msg, &self.completed) {
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
                RoutingDecision::Process {
                    e3_id,
                    post_forward,
                } => {
                    let repositories = self.repository().repositories();
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

                    self.checkpoint();
                    Ok(())
                }
            }
            },
        );
    }
}
