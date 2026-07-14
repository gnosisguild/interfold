// SPDX-License-Identifier: LGPL-3.0-only

//! Execute workflow effects through the event bus and owned Actix timers.

use super::*;

impl AccusationManager {
    /// Perform the I/O the [`AccusationVoting`] service requested.
    ///
    /// This is the *only* place the actor publishes events or touches timers —
    /// keeping all protocol decisions in the pure service.
    pub(in crate::actors::accusation_manager) fn apply_actions(
        &mut self,
        actions: Vec<VoteAction>,
        ctx: &mut Context<Self>,
    ) {
        for action in actions {
            match action {
                VoteAction::PublishAccusation {
                    accusation,
                    ec,
                    dedup_key,
                } => {
                    if let Err(err) = self.bus.publish(accusation, ec) {
                        error!("Failed to broadcast ProofFailureAccusation: {err}");
                        // Preserve the original rollback: re-allow this
                        // (accused, proof_type) accusation on a dead bus.
                        self.voting.rollback_initiation(&dedup_key);
                    }
                }
                VoteAction::PublishVote { vote, ec } => {
                    if let Err(err) = self.bus.publish(vote, ec) {
                        error!("Failed to broadcast AccusationVote: {err}");
                    }
                }
                VoteAction::PublishQuorum { quorum, ec } => {
                    if let Err(err) = self.bus.publish(quorum, ec) {
                        error!("Failed to publish AccusationQuorumReached: {err}");
                    }
                }
                VoteAction::DispatchZk {
                    request,
                    ec,
                    correlation_id,
                } => {
                    if let Err(err) = self.bus.publish(request, ec) {
                        error!("Failed to dispatch C3a/C3b ZK re-verification: {err}");
                        self.voting.discard_reverification(&correlation_id);
                    }
                }
                VoteAction::StartTimeout(accusation_id) => {
                    let timeout = self.voting.vote_timeout();
                    let handle = ctx.run_later(timeout, move |act, _ctx| {
                        act.timeout_handles.remove(&accusation_id);
                        if let Some((quorum, ec)) = act.voting.on_vote_timeout(accusation_id) {
                            if let Err(err) = act.bus.publish(quorum, ec) {
                                error!(
                                    "Failed to publish AccusationQuorumReached on timeout: {err}"
                                );
                            }
                        }
                    });
                    self.timeout_handles.insert(accusation_id, handle);
                }
                VoteAction::CancelTimeout(accusation_id) => {
                    if let Some(handle) = self.timeout_handles.remove(&accusation_id) {
                        ctx.cancel_future(handle);
                    }
                }
            }
        }
    }
}
