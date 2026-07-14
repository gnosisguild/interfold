// SPDX-License-Identifier: LGPL-3.0-only

//! Committee deadline, cleanup, and lifecycle handlers.

use super::*;

impl Handler<InterfoldEvent> for CommitteeFinalizer {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        let (msg, ec) = msg.into_components();
        match msg {
            InterfoldEventData::CommitteeRequested(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::EffectsEnabled(data) => self.notify_sync(ctx, data),
            InterfoldEventData::TicketGenerated(data) => self.notify_sync(ctx, data),
            InterfoldEventData::Shutdown(data) => self.notify_sync(ctx, data),
            InterfoldEventData::E3Failed(data) => self.notify_sync(ctx, TypedEvent::new(data, ec)),
            InterfoldEventData::E3RequestComplete(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::E3StageChanged(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            _ => (),
        }
    }
}

impl Handler<TypedEvent<CommitteeRequested>> for CommitteeFinalizer {
    type Result = ();

    // TODO: Remove all async from this function. Remove reliance on e3_evm package. Add unit test.
    fn handle(
        &mut self,
        msg: TypedEvent<CommitteeRequested>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let e3_id = msg.e3_id.to_string();
        self.pending_requests.insert(
            e3_id.clone(),
            PendingCommitteeRequest {
                e3_id: msg.e3_id.clone(),
                committee_deadline: msg.committee_deadline,
                ec: msg.get_ctx().clone(),
            },
        );
        self.schedule_if_ready(&e3_id, ctx);
    }
}

impl Handler<TicketGenerated> for CommitteeFinalizer {
    type Result = ();

    fn handle(&mut self, msg: TicketGenerated, ctx: &mut Self::Context) -> Self::Result {
        let Some(party_index) = msg.party_index else {
            return;
        };

        let e3_id = msg.e3_id.to_string();
        self.party_indexes.insert(e3_id.clone(), party_index);
        self.schedule_if_ready(&e3_id, ctx);
    }
}

impl Handler<EffectsEnabled> for CommitteeFinalizer {
    type Result = ();

    fn handle(&mut self, _msg: EffectsEnabled, ctx: &mut Self::Context) -> Self::Result {
        self.effects_enabled = true;
        let e3_ids: Vec<String> = self.pending_requests.keys().cloned().collect();
        for e3_id in e3_ids {
            self.schedule_if_ready(&e3_id, ctx);
        }
    }
}

impl Handler<Shutdown> for CommitteeFinalizer {
    type Result = ();
    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        info!("Killing CommitteeFinalizer");
        // Cancel all pending finalization tasks
        for (_, handle) in self.pending_committees.drain() {
            ctx.cancel_future(handle);
        }
        ctx.stop();
    }
}

impl Handler<TypedEvent<E3Failed>> for CommitteeFinalizer {
    type Result = ();
    fn handle(&mut self, msg: TypedEvent<E3Failed>, ctx: &mut Self::Context) -> Self::Result {
        let e3_id_str = msg.e3_id.to_string();
        if let Some(handle) = self.pending_committees.remove(&e3_id_str) {
            info!(
                e3_id = %msg.e3_id,
                reason = ?msg.reason,
                "E3 failed — cancelling pending committee finalization timer"
            );
            ctx.cancel_future(handle);
        }
        self.pending_requests.remove(&e3_id_str);
        self.party_indexes.remove(&e3_id_str);
    }
}

impl Handler<TypedEvent<E3StageChanged>> for CommitteeFinalizer {
    type Result = ();
    fn handle(&mut self, msg: TypedEvent<E3StageChanged>, ctx: &mut Self::Context) -> Self::Result {
        match &msg.new_stage {
            E3Stage::Complete | E3Stage::Failed => {
                let e3_id_str = msg.e3_id.to_string();
                if let Some(handle) = self.pending_committees.remove(&e3_id_str) {
                    info!(
                        e3_id = %msg.e3_id,
                        stage = ?msg.new_stage,
                        "E3 reached terminal stage — cancelling pending committee finalization timer"
                    );
                    ctx.cancel_future(handle);
                }
                self.pending_requests.remove(&e3_id_str);
                self.party_indexes.remove(&e3_id_str);
            }
            _ => {}
        }
    }
}

impl Handler<TypedEvent<E3RequestComplete>> for CommitteeFinalizer {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<E3RequestComplete>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let e3_id_str = msg.e3_id.to_string();
        if let Some(handle) = self.pending_committees.remove(&e3_id_str) {
            ctx.cancel_future(handle);
        }
        self.pending_requests.remove(&e3_id_str);
        self.party_indexes.remove(&e3_id_str);
    }
}
