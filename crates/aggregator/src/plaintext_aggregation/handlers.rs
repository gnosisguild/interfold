// SPDX-License-Identifier: LGPL-3.0-only

//! Actix lifecycle, timeout ownership, and message routing.

use super::*;

impl Actor for ThresholdPlaintextAggregator {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
        self.arm_collection_timeout(ctx);
    }
}

impl Handler<DecryptionCollectionTimeout> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(&mut self, _: DecryptionCollectionTimeout, ctx: &mut Self::Context) -> Self::Result {
        self.pending.timeout_handle = None;

        if !self.is_aggregator {
            debug!(
                e3_id = %self.e3_id,
                "Ignoring a stale decryption-share timeout after aggregator demotion"
            );
            return;
        }

        // Only fail while still collecting shares; once we have transitioned past `Collecting`
        // (VerifyingC6/Computing/…) the round is progressing and the timer is a no-op.
        let Some(ThresholdPlaintextAggregatorState::Collecting(collecting)) = self.state.get()
        else {
            debug!(
                e3_id = %self.e3_id,
                "Decryption-share collection timeout fired but round already progressed past collection; ignoring"
            );
            return;
        };

        let collected = collecting.shares.len();
        let required = self.aggregated_committee_n();
        warn!(
            e3_id = %self.e3_id,
            collected,
            required,
            "Decryption-share collection timed out with {collected}/{required} honest shares; failing E3 round (DecryptionTimeout)"
        );

        let Some(ec) = self.pending.timeout_ec.clone() else {
            warn!(
                e3_id = %self.e3_id,
                "No event context captured for decryption timeout; cannot emit E3Failed. Stopping aggregator."
            );
            ctx.stop();
            return;
        };

        if let Err(e) = self.bus.publish(
            E3Failed {
                e3_id: self.e3_id.clone(),
                failed_at_stage: E3Stage::CiphertextReady,
                reason: FailureReason::DecryptionTimeout,
            },
            ec,
        ) {
            warn!(
                e3_id = %self.e3_id,
                error = %e,
                "Failed to publish E3Failed on decryption-share collection timeout"
            );
        }

        ctx.stop();
    }
}

impl Handler<InterfoldEvent> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        let (msg, ec) = msg.into_components();
        match msg {
            InterfoldEventData::DecryptionshareCreated(data) => {
                ctx.notify(TypedEvent::new(data, ec))
            }
            InterfoldEventData::E3RequestComplete(_) => self.notify_sync(ctx, Die),
            InterfoldEventData::ComputeResponse(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::ComputeRequestError(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::CommitteeMemberExpelled(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::CommitteeMemberExcluded(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::ShareVerificationComplete(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::AggregationProofSigned(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::AggregatorChanged(data) => {
                self.notify_sync(ctx, TypedEvent::new(data, ec))
            }
            InterfoldEventData::EffectsEnabled(_) => {
                trap(EType::PlaintextAggregation, &self.bus.with_ec(&ec), || {
                    self.resume_in_flight_work(ec)
                });
            }
            _ => (),
        }
    }
}

impl Handler<TypedEvent<AggregatorChanged>> for ThresholdPlaintextAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<AggregatorChanged>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if msg.e3_id != self.e3_id || msg.is_aggregator == self.is_aggregator {
            return;
        }
        self.is_aggregator = msg.is_aggregator;
        if self.is_aggregator {
            self.arm_collection_timeout(ctx);
        } else {
            self.cancel_collection_timeout(ctx);
        }
    }
}

impl Handler<TypedEvent<DecryptionshareCreated>> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(
        &mut self,
        msg: TypedEvent<DecryptionshareCreated>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PublickeyAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || {
                let Some(ThresholdPlaintextAggregatorState::Collecting(Collecting { .. })) =
                    self.state.get()
                else {
                    debug!(state=?self.state, "Aggregator has been closed for collecting so ignoring this event.");
                    return Ok(());
                };
                let node = msg.node.clone();
                let e3_id = msg.e3_id.clone();
                let request = E3CommitteeContainsRequest::new(e3_id, node, msg, ctx.address());
                self.sortition.try_send(request)?;
                Ok(())
            },
        )
    }
}

impl Handler<E3CommitteeContainsResponse<TypedEvent<DecryptionshareCreated>>>
    for ThresholdPlaintextAggregator
{
    type Result = ();
    fn handle(
        &mut self,
        msg: E3CommitteeContainsResponse<TypedEvent<DecryptionshareCreated>>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PublickeyAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || {
                let e3_id = &msg.e3_id;
                if *e3_id != self.e3_id {
                    bail!("Wrong e3_id sent to aggregator. This should not happen.")
                };

                if !msg.is_found_in_committee() {
                    trace!("Node {} not found in finalized committee", &msg.node);
                    return Ok(());
                };
                if !self.node_owns_aggregated_pk_party_slot(&msg.node, msg.party_id) {
                    trace!(
                        "Node {} does not own honest party slot {} — ignoring decryption share",
                        &msg.node,
                        msg.party_id
                    );
                    return Ok(());
                }

                // Trust the party_id from the event - it's based on CommitteeFinalized order
                // which is the authoritative source of truth for party IDs
                let (
                    DecryptionshareCreated {
                        party_id,
                        decryption_share,
                        signed_decryption_proofs,
                        ..
                    },
                    ec,
                ) = msg.into_inner().into_components();

                // Capture the latest context so a subsequent collection timeout can emit
                // `E3Failed` with a sensible causal parent.
                self.pending.timeout_ec = Some(ec.clone());
                self.add_share(party_id, decryption_share, signed_decryption_proofs, &ec)?;

                // If we transitioned to VerifyingC6, dispatch C6 verification
                // using the proofs persisted in state
                if let Some(ThresholdPlaintextAggregatorState::VerifyingC6(ref state)) =
                    self.state.get()
                {
                    self.dispatch_c6_verification(state.c6_proofs.clone(), ec)?;
                }

                Ok(())
            },
        )
    }
}

impl Handler<TypedEvent<ComputeResponse>> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(
        &mut self,
        msg: TypedEvent<ComputeResponse>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PlaintextAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || self.handle_compute_response(msg, ctx),
        )
    }
}

impl Handler<TypedEvent<ComputeRequestError>> for ThresholdPlaintextAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ComputeRequestError>,
        _: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PlaintextAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || self.handle_compute_request_error(msg),
        )
    }
}

impl Handler<TypedEvent<CommitteeMemberExpelled>> for ThresholdPlaintextAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<CommitteeMemberExpelled>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PlaintextAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || {
                let (msg, ec) = msg.into_components();
                let Some(party_id) = msg.party_id else {
                    return Ok(());
                };

                self.handle_member_expelled(party_id, &ec)?;

                if let Some(ThresholdPlaintextAggregatorState::VerifyingC6(ref state)) =
                    self.state.get()
                {
                    self.dispatch_c6_verification(state.c6_proofs.clone(), ec)?;
                }

                Ok(())
            },
        )
    }
}

impl Handler<TypedEvent<CommitteeMemberExcluded>> for ThresholdPlaintextAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<CommitteeMemberExcluded>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PlaintextAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || {
                let (msg, ec) = msg.into_components();
                let Some(party_id) = msg.party_id else {
                    return Ok(());
                };

                self.handle_member_expelled(party_id, &ec)?;
                if let Some(ThresholdPlaintextAggregatorState::VerifyingC6(ref state)) =
                    self.state.get()
                {
                    self.dispatch_c6_verification(state.c6_proofs.clone(), ec)?;
                }
                Ok(())
            },
        )
    }
}

impl Handler<TypedEvent<ShareVerificationComplete>> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(
        &mut self,
        msg: TypedEvent<ShareVerificationComplete>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PlaintextAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || self.handle_c6_verification_complete(msg),
        )
    }
}

impl Handler<TypedEvent<AggregationProofSigned>> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(
        &mut self,
        msg: TypedEvent<AggregationProofSigned>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        trap(
            EType::PlaintextAggregation,
            &self.bus.with_ec(msg.get_ctx()),
            || self.handle_aggregation_proof_signed(msg, ctx),
        )
    }
}

impl Handler<Die> for ThresholdPlaintextAggregator {
    type Result = ();
    fn handle(&mut self, _: Die, ctx: &mut Self::Context) -> Self::Result {
        self.cancel_collection_timeout(ctx);
        ctx.stop()
    }
}

#[cfg(test)]
#[derive(Message)]
#[rtype(result = "bool")]
pub(super) struct CollectionTimeoutArmed;

#[cfg(test)]
impl Handler<CollectionTimeoutArmed> for ThresholdPlaintextAggregator {
    type Result = bool;

    fn handle(&mut self, _: CollectionTimeoutArmed, _: &mut Self::Context) -> Self::Result {
        self.pending.timeout_handle.is_some()
    }
}
