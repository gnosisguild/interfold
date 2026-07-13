// SPDX-License-Identifier: LGPL-3.0-only

//! Mailbox entry points and lifecycle hooks.

use super::*;

#[derive(Message)]
#[rtype(result = "()")]
struct IncomingNetEvent(NetEvent);

#[derive(Message)]
#[rtype(result = "()")]
struct NetInputLagged(u64);

impl Actor for NetEventBuffer {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
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
