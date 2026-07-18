// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Startup composition for the global ZK actor system.

use actix::{Actor, Addr};
use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use e3_events::{BusHandle, Committee, E3id};
use e3_request::E3Meta;
use std::collections::HashMap;

use crate::actors::{
    NodeProofAggregator, ProofRequestActor, ProofVerificationActor, ShareVerificationActor, ZkActor,
};
use crate::ZkBackend;

/// Durable inputs needed by global proof-verification actors before EventStore replay begins.
///
/// Both maps are projections of canonical protocol events. They are startup seeds, not separate
/// authorities: live or replayed lifecycle events continue to update the actor caches.
#[derive(Clone, Debug, Default)]
pub struct ZkActorRecovery {
    finalized_committees: HashMap<E3id, Committee>,
    e3_metadata: HashMap<E3id, E3Meta>,
}

impl ZkActorRecovery {
    pub fn new(
        finalized_committees: HashMap<E3id, Committee>,
        e3_metadata: HashMap<E3id, E3Meta>,
    ) -> Self {
        Self {
            finalized_committees,
            e3_metadata,
        }
    }
}

/// Setup all ZK-related actors.
///
/// Requires a `ZkBackend` for proof generation/verification and a `PrivateKeySigner` for signing
/// proofs. `dkg_fold_attestation_verifiers_by_chain` maps each enabled chain to its EIP-712
/// verifying contract. `recovery` seeds global verifier context before EventStore replay.
pub fn setup_zk_actors(
    bus: &BusHandle,
    backend: &ZkBackend,
    signer: PrivateKeySigner,
    dkg_fold_attestation_verifiers_by_chain: HashMap<u64, Option<Address>>,
    recovery: ZkActorRecovery,
    proof_aggregation_enabled: bool,
) -> ZkActors {
    let ZkActorRecovery {
        finalized_committees,
        e3_metadata,
    } = recovery;
    let zk_actor = ZkActor::new(backend).start();
    let verifier = zk_actor.clone().recipient();

    let proof_request = ProofRequestActor::setup(bus, signer.clone(), proof_aggregation_enabled);
    let proof_verification =
        ProofVerificationActor::setup(bus, verifier, finalized_committees.clone(), e3_metadata);
    let share_verification = ShareVerificationActor::setup(bus, finalized_committees);
    let node_proof_aggregator = NodeProofAggregator::setup(
        bus,
        signer,
        dkg_fold_attestation_verifiers_by_chain,
        proof_aggregation_enabled,
    );

    ZkActors {
        zk_actor,
        proof_request,
        proof_verification,
        share_verification,
        node_proof_aggregator,
    }
}

/// Container for all ZK actor addresses.
pub struct ZkActors {
    pub zk_actor: Addr<ZkActor>,
    pub proof_request: Addr<ProofRequestActor>,
    pub proof_verification: Addr<ProofVerificationActor>,
    pub share_verification: Addr<ShareVerificationActor>,
    pub node_proof_aggregator: Addr<NodeProofAggregator>,
}
