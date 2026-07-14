// SPDX-License-Identifier: LGPL-3.0-only

//! Event routing and compute-result handling.

use super::*;

impl Actor for NodeProofAggregator {
    type Context = Context<Self>;
}

impl Handler<InterfoldEvent> for NodeProofAggregator {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, _ctx: &mut Self::Context) -> Self::Result {
        let (data, ec) = msg.into_components();
        match data {
            InterfoldEventData::ThresholdSharePending(data) => {
                self.handle_threshold_share_pending(TypedEvent::new(data, ec));
            }
            InterfoldEventData::DKGInnerProofReady(data) => {
                self.handle_inner_proof_ready(TypedEvent::new(data, ec));
            }
            InterfoldEventData::ComputeResponse(data) => {
                self.handle_compute_response(TypedEvent::new(data, ec));
            }
            InterfoldEventData::ComputeRequestError(data) => {
                self.handle_compute_request_error(TypedEvent::new(data, ec));
            }
            _ => {}
        }
    }
}

impl Handler<TypedEvent<ThresholdSharePending>> for NodeProofAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ThresholdSharePending>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.handle_threshold_share_pending(msg);
    }
}

impl Handler<TypedEvent<DKGInnerProofReady>> for NodeProofAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<DKGInnerProofReady>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.handle_inner_proof_ready(msg);
    }
}

impl Handler<TypedEvent<ComputeResponse>> for NodeProofAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ComputeResponse>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.handle_compute_response(msg);
    }
}

impl Handler<TypedEvent<ComputeRequestError>> for NodeProofAggregator {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ComputeRequestError>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.handle_compute_request_error(msg);
    }
}

impl NodeProofAggregator {
    pub(super) fn dkg_fold_attestation_verifier_for(&self, e3_id: &E3id) -> Option<Address> {
        let chain_id = e3_id.chain_id();
        match self.dkg_fold_attestation_verifiers_by_chain.get(&chain_id) {
            Some(Some(addr)) => Some(*addr),
            Some(None) => None,
            None => {
                warn!(
                    chain_id,
                    "no dkgFoldAttestationVerifier configured for chain"
                );
                None
            }
        }
    }

    pub(super) fn handle_compute_response(&mut self, msg: TypedEvent<ComputeResponse>) {
        let (msg, _ec) = msg.into_components();
        if let ComputeResponseKind::Zk(ZkResponse::NodeDkgFold(resp)) = msg.response {
            self.handle_node_dkg_response(&msg.correlation_id, resp.proof);
        }
    }

    pub(super) fn handle_compute_request_error(&mut self, msg: TypedEvent<ComputeRequestError>) {
        let (msg, ec) = msg.into_components();
        if let Some(e3_id) = self.fold_correlation.remove(msg.correlation_id()) {
            error!(
                "NodeProofAggregator: NodeDkgFold failed for E3 {}: {:?} — aggregation aborted",
                e3_id,
                msg.get_err()
            );
            let state = self.states.remove(&e3_id);
            warn!(
                "NodeProofAggregator: E3 {} NodeDkgFold failed — publishing E3Failed",
                e3_id
            );

            if let Some(_state) = state {
                if let Err(err) = self.bus.publish(
                    E3Failed {
                        e3_id: e3_id.clone(),
                        failed_at_stage: E3Stage::CommitteeFinalized,
                        reason: FailureReason::DKGInvalidShares,
                    },
                    ec,
                ) {
                    error!(
                        "NodeProofAggregator: failed to publish E3Failed for E3 {}: {err}",
                        e3_id
                    );
                }
            }
        }
    }
}
