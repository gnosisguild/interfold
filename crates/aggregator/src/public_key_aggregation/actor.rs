// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::domain::committee::committee_addresses_in_party_order;
use crate::workflow::publickey_aggregation::{
    check_c1_keyshare_commitments, extract_pk_commitment, verify_dkg_fold_attestation, C1Dispatch,
    HonestSelection, PublicKeyAggregation,
};
use actix::prelude::*;
use anyhow::Result;
use e3_data::Persistable;
use e3_events::DkgFoldAttestationContext;
use e3_events::{
    prelude::*, BusHandle, ComputeRequest, ComputeRequestError, ComputeResponse,
    ComputeResponseKind, CorrelationId, DKGRecursiveAggregationComplete, Die,
    DkgAggregationRequest, E3Failed, E3Stage, E3id, EventContext, FailureReason, InterfoldEvent,
    InterfoldEventData, KeyshareCreated, NodesFoldStepRequest, OrderedSet,
    PkAggregationProofPending, PkAggregationProofRequest, PkAggregationProofSigned, Proof,
    ProofType, PublicKeyAggregated, Sequenced, ShareVerificationComplete,
    ShareVerificationDispatched, SignedProofFailed, SignedProofPayload, TypedEvent,
    VerificationKind, ZkRequest, ZkResponse,
};
use e3_events::{trap, EType};
use e3_fhe::{Fhe, GetAggregatePublicKey};
use e3_fhe_params::BfvPreset;
use e3_utils::NotifySync;
use e3_utils::{ArcBytes, MAILBOX_LIMIT};
use e3_zk_helpers::CiphernodesCommitteeSize;
use std::sync::Arc;
use tracing::{error, info, warn};

// Public-key aggregation state machine + pure transition logic now live in
// `crate::workflow::publickey_aggregation`; re-exported here to preserve the public path
// `e3_aggregator::publickey_aggregator::PublicKeyAggregatorState`.
pub use crate::workflow::publickey_aggregation::PublicKeyAggregatorState;

pub struct PublicKeyAggregator {
    fhe: Arc<Fhe>,
    bus: BusHandle,
    e3_id: E3id,
    state: Persistable<PublicKeyAggregatorState>,
    params_preset: BfvPreset,
    committee_size: CiphernodesCommitteeSize,
    dkg_fold_attestation_context: Option<DkgFoldAttestationContext>,
    /// DKG recursive aggregation events received before entering GeneratingC5Proof.
    early_dkg_proofs: Vec<TypedEvent<DKGRecursiveAggregationComplete>>,
}

pub struct PublicKeyAggregatorParams {
    pub fhe: Arc<Fhe>,
    pub bus: BusHandle,
    pub e3_id: E3id,
    pub params_preset: BfvPreset,
    pub committee_size: CiphernodesCommitteeSize,
    pub dkg_fold_attestation_context: Option<DkgFoldAttestationContext>,
}

/// Aggregate PublicKey for a committee of nodes. This actor listens for KeyshareCreated events
/// around a particular e3_id, verifies C1 proofs, aggregates the public key, generates a C5
/// proof of correct aggregation, and broadcasts a PublicKeyAggregated event on the event bus.
impl PublicKeyAggregator {
    pub fn new(
        params: PublicKeyAggregatorParams,
        state: Persistable<PublicKeyAggregatorState>,
    ) -> Self {
        PublicKeyAggregator {
            fhe: params.fhe,
            bus: params.bus,
            e3_id: params.e3_id,
            state,
            params_preset: params.params_preset,
            committee_size: params.committee_size,
            dkg_fold_attestation_context: params.dkg_fold_attestation_context,
            early_dkg_proofs: Vec::new(),
        }
    }
}

#[path = "effects/mod.rs"]
mod effects;
#[path = "handlers.rs"]
mod handlers;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
