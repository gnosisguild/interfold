// SPDX-License-Identifier: LGPL-3.0-only

//! Actix envelope routing into the accusation workflow.

use super::*;

impl Actor for AccusationManager {
    type Context = Context<Self>;
}

impl Handler<InterfoldEvent> for AccusationManager {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        let (msg, ec) = msg.into_components();
        match msg {
            InterfoldEventData::ProofVerificationFailed(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::ProofVerificationPassed(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::ProofFailureAccusation(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::AccusationVote(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::ComputeResponse(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::ComputeRequestError(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::SlashExecuted(data) => {
                self.voting.on_slash_executed(data);
            }
            InterfoldEventData::CommitmentConsistencyViolation(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            _ => (),
        }
    }
}

impl Handler<TypedEvent<ProofVerificationFailed>> for AccusationManager {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ProofVerificationFailed>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let (data, ec) = msg.into_components();
        let actions = self.voting.on_local_proof_failure(data, &ec);
        self.apply_actions(actions, ctx);
    }
}

impl Handler<TypedEvent<ProofVerificationPassed>> for AccusationManager {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ProofVerificationPassed>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let (data, _ec) = msg.into_components();
        self.voting.on_proof_verification_passed(data);
    }
}

impl Handler<TypedEvent<ProofFailureAccusation>> for AccusationManager {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ProofFailureAccusation>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let (data, ec) = msg.into_components();
        let actions = self.voting.on_accusation_received(data, &ec);
        self.apply_actions(actions, ctx);
    }
}

impl Handler<TypedEvent<AccusationVote>> for AccusationManager {
    type Result = ();

    fn handle(&mut self, msg: TypedEvent<AccusationVote>, ctx: &mut Self::Context) -> Self::Result {
        let (data, ec) = msg.into_components();
        let actions = self.voting.on_vote_received(data, &ec);
        self.apply_actions(actions, ctx);
    }
}

impl Handler<TypedEvent<ComputeResponse>> for AccusationManager {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ComputeResponse>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let actions = self.voting.handle_reverification_response(msg);
        self.apply_actions(actions, ctx);
    }
}

impl Handler<TypedEvent<ComputeRequestError>> for AccusationManager {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ComputeRequestError>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.voting.handle_reverification_error(msg);
    }
}

impl Handler<TypedEvent<CommitmentConsistencyViolation>> for AccusationManager {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<CommitmentConsistencyViolation>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let (data, ec) = msg.into_components();
        let actions = self.voting.on_consistency_violation(data, &ec);
        self.apply_actions(actions, ctx);
    }
}
