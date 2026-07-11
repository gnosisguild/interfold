// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Actor-based components for ZK proof generation and verification.
//!
//! ## Architecture
//!
//! This module follows a clean separation between core business logic and IO operations:
//!
//! ### Core Actors (Business Logic - No IO)
//! - [`ProofRequestActor`]: Converts `EncryptionKeyPending` → `ComputeRequest` and handles responses
//! - [`ProofVerificationActor`]: Verifies `EncryptionKeyReceived` and converts to `EncryptionKeyCreated`
//! - [`ShareVerificationActor`]: Handles ECDSA + ZK verification for C2/C3/C4 share proofs
//!
//! ### IO Actors (File System Operations)
//! - [`ZkActor`]: Performs actual proof generation/verification using disk-based circuits and bb binary
//!
//! ## Usage
//!
//! ```rust,ignore
//! use e3_zk_prover::{ZkActorRecovery, ZkBackend, setup_zk_actors};
//! use e3_events::BusHandle;
//! use alloy::signers::local::PrivateKeySigner;
//! use std::collections::HashMap;
//!
//! let bus = BusHandle::default();
//! let backend = ZkBackend::with_default_dir().await?;
//! let signer = PrivateKeySigner::random();
//!
//! // Setup all actors with proper separation of concerns
//! setup_zk_actors(
//!     &bus,
//!     &backend,
//!     signer,
//!     HashMap::new(),
//!     ZkActorRecovery::default(),
//! );
//! ```

pub mod accusation_manager;
pub mod accusation_manager_ext;
pub mod commitment_consistency_checker;
pub mod commitment_consistency_checker_ext;
pub mod commitment_links;
pub mod node_proof_aggregator;
pub mod proof_request;
pub mod proof_verification;
pub mod share_verification;
pub mod zk_actor;

// Re-export accusation types from their canonical home in e3-slashing.
pub use e3_slashing::CommitmentConsistencyCheckerExtension;
pub use node_proof_aggregator::NodeProofAggregator;
pub use proof_request::ProofRequestActor;
pub use proof_verification::{
    ProofVerificationActor, ZkVerificationRequest, ZkVerificationResponse,
};
pub use share_verification::ShareVerificationActor;
pub use zk_actor::ZkActor;

use actix::{Actor, Addr};
use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use e3_events::{BusHandle, Committee, E3id};
use e3_request::E3Meta;
use std::collections::HashMap;

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

/// Setup all ZK-related actors with proper separation of concerns.
///
/// Requires a `ZkBackend` for proof generation/verification and a
/// `PrivateKeySigner` for signing proofs (fault attribution).
/// `dkg_fold_attestation_verifiers_by_chain` maps each enabled chain's id to
/// `CiphernodeRegistry.dkgFoldAttestationVerifier()` (EIP-712 `verifyingContract`
/// for fold attestations). Fetched at node startup when proof aggregation is enabled.
/// `recovery` seeds global verifier context before EventStore replay.
pub fn setup_zk_actors(
    bus: &BusHandle,
    backend: &ZkBackend,
    signer: PrivateKeySigner,
    dkg_fold_attestation_verifiers_by_chain: HashMap<u64, Option<Address>>,
    recovery: ZkActorRecovery,
) -> ZkActors {
    let ZkActorRecovery {
        finalized_committees,
        e3_metadata,
    } = recovery;
    let zk_actor = ZkActor::new(backend).start();
    let verifier = zk_actor.clone().recipient();

    let proof_request = ProofRequestActor::setup(bus, signer.clone());
    let proof_verification =
        ProofVerificationActor::setup(bus, verifier, finalized_committees.clone(), e3_metadata);
    let share_verification = ShareVerificationActor::setup(bus, finalized_committees);
    let node_proof_aggregator =
        NodeProofAggregator::setup(bus, signer, dkg_fold_attestation_verifiers_by_chain);

    ZkActors {
        zk_actor,
        proof_request,
        proof_verification,
        share_verification,
        node_proof_aggregator,
    }
}

/// Container for all ZK-related actor addresses.
pub struct ZkActors {
    pub zk_actor: Addr<ZkActor>,
    pub proof_request: Addr<ProofRequestActor>,
    pub proof_verification: Addr<ProofVerificationActor>,
    pub share_verification: Addr<ShareVerificationActor>,
    pub node_proof_aggregator: Addr<NodeProofAggregator>,
}
