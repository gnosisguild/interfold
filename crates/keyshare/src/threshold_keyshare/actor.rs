// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::prelude::*;
use anyhow::{anyhow, bail, Context, Result};
use e3_crypto::{Cipher, SensitiveBytes};
use e3_data::Persistable;
use e3_events::{
    prelude::*, trap, BusHandle, CiphernodeSelected, CiphertextOutputPublished,
    CommitteeMemberExpelled, ComputeRequest, ComputeResponse, ComputeResponseKind, CorrelationId,
    DecryptionKeyShared, DecryptionShareProofSigned, DecryptionShareProofsPending, Die,
    DkgProofSigned, DkgShareDecryptionProofRequest, E3Failed, E3RequestComplete, E3Stage, EType,
    EncryptionKey, EncryptionKeyCollectionFailed, EncryptionKeyCreated, EncryptionKeyPending,
    EventContext, FailureReason, InterfoldEvent, InterfoldEventData, KeyshareCreated,
    PartyProofsToVerify, PartyShareDecryptionProofsToVerify, PkGenerationProofSigned, ProofType,
    Sequenced, ShareDecryptionProofPending, ShareVerificationComplete, ShareVerificationDispatched,
    SignedProofPayload, ThresholdShare, ThresholdShareCollectionFailed, ThresholdShareCreated,
    ThresholdShareDecryptionProofRequest, ThresholdSharePending, TypedEvent, VerificationKind,
};
use e3_fhe_params::create_deterministic_crp_from_default_seed;
use e3_fhe_params::BfvPreset;
use e3_trbfv::{
    calculate_decryption_key::CalculateDecryptionKeyResponse,
    calculate_decryption_share::{
        CalculateDecryptionShareRequest, CalculateDecryptionShareResponse,
    },
    gen_esi_sss::{GenEsiSssRequest, GenEsiSssResponse},
    gen_pk_share_and_sk_sss::{GenPkShareAndSkSssRequest, GenPkShareAndSkSssResponse},
    shares::SharedSecret,
    TrBFVConfig, TrBFVRequest, TrBFVResponse,
};
use e3_utils::utility_types::ArcBytes;
use e3_utils::{NotifySync, MAILBOX_LIMIT};
use e3_zk_helpers::CiphernodesCommitteeSize;
use fhe_traits::Serialize;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};
use tracing::{error, info, trace, warn};

use crate::actors::decryption_key_shared_collector::{
    AllDecryptionKeySharesCollected, DecryptionKeySharedCollectionFailed,
    DecryptionKeySharedCollector, ExpelPartyFromDecryptionKeySharedCollection,
};
use crate::actors::encryption_key_collector::{
    AllEncryptionKeysCollected, EncryptionKeyCollector, ExpelPartyFromKeyCollection,
};
use crate::actors::threshold_share_collector::{
    ExpelPartyFromShareCollection, ThresholdShareCollector,
};
use crate::domain::timeout_policy::{resolve_timeout, DkgTimeoutPhase};
use crate::domain::{
    build_decryption_key_plan, build_shares_generated_plan, generate_bfv_keypair,
    AggregatingDecryptionKey, BfvKeypairMaterial, CollectingEncryptionKeysData, Decrypting,
    DecryptionKeyPlan, GeneratingDecryptionProof, GeneratingThresholdShareData, KeyshareState,
    ProofRequestData, ReadyForDecryption, ReceivedShareProofs, ThresholdKeyshareState,
};

#[derive(Message, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[rtype(result = "()")]
pub struct GenPkShareAndSkSss(CiphernodeSelected);

#[derive(Message, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[rtype(result = "()")]
pub struct GenEsiSss {
    pub ciphernode_selected: CiphernodeSelected,
    pub e_sm_raw: SensitiveBytes,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AllThresholdSharesCollected {
    /// Threshold shares sorted by ascending `party_id`.
    shares: Vec<Arc<ThresholdShare>>,
    /// Proofs from each sender, ordered by party_id (parallel to shares).
    share_proofs: Vec<ReceivedShareProofs>,
}

impl AllThresholdSharesCollected {
    pub fn new(
        shares: HashMap<u64, Arc<ThresholdShare>>,
        proofs: HashMap<u64, ReceivedShareProofs>,
    ) -> Self {
        let mut entries: Vec<_> = shares.into_iter().collect();
        entries.sort_by_key(|(k, _)| *k);
        let (party_ids, shares): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
        let share_proofs = party_ids
            .iter()
            .map(|pid| {
                proofs.get(pid).cloned().unwrap_or(ReceivedShareProofs {
                    signed_c2a_proof: None,
                    signed_c2b_proof: None,
                    signed_c3a_proofs: Vec::new(),
                    signed_c3b_proofs: Vec::new(),
                })
            })
            .collect();
        Self {
            shares,
            share_proofs,
        }
    }
}

pub struct ThresholdKeyshareParams {
    pub bus: BusHandle,
    pub cipher: Arc<Cipher>,
    pub state: Persistable<ThresholdKeyshareState>,
    pub share_enc_preset: BfvPreset,
}

/// Ephemeral bridge data for operations already represented by the persisted keyshare phase.
///
/// Keeping these values behind one boundary makes their lifetime and recovery status explicit. A
/// recovery-safe replacement can move this bundle to a versioned workflow/outbox record without
/// growing the actor itself again.
#[derive(Default)]
struct PendingKeyshareWork {
    /// Shares awaiting the C2/C3 verification result.
    shares: Vec<Arc<ThresholdShare>>,
    /// C4 requests awaiting the threshold-decryption-key result.
    share_decryption_data: Option<(
        DkgShareDecryptionProofRequest,
        Vec<DkgShareDecryptionProofRequest>,
    )>,
    /// Peer C4 artifacts awaiting verification.
    c4_verification_shares: Option<HashMap<u64, DecryptionKeyShared>>,
    /// Own plaintext DKG shares awaiting the aggregation transition.
    own_dkg_shares: Option<(SensitiveBytes, Vec<SensitiveBytes>)>,
    /// C4 completed before the signed C1 artifact became available.
    keyshare_publish: bool,
}

pub struct ThresholdKeyshare {
    bus: BusHandle,
    cipher: Arc<Cipher>,
    decryption_key_collector: Option<Addr<ThresholdShareCollector>>,
    encryption_key_collector: Option<Addr<EncryptionKeyCollector>>,
    decryption_key_shared_collector: Option<Addr<DecryptionKeySharedCollector>>,
    state: Persistable<ThresholdKeyshareState>,
    share_enc_preset: BfvPreset,
    pending: PendingKeyshareWork,
}

impl ThresholdKeyshare {
    pub fn new(params: ThresholdKeyshareParams) -> Self {
        Self {
            bus: params.bus,
            cipher: params.cipher,
            decryption_key_collector: None,
            encryption_key_collector: None,
            decryption_key_shared_collector: None,
            state: params.state,
            share_enc_preset: params.share_enc_preset,
            pending: PendingKeyshareWork::default(),
        }
    }

    fn store_signed_pk_generation_proof(
        &mut self,
        ec: &EventContext<Sequenced>,
        signed: SignedProofPayload,
    ) -> Result<()> {
        self.state.try_mutate(ec, |mut s| {
            match &mut s.state {
                KeyshareState::AggregatingDecryptionKey(adk) => {
                    adk.signed_pk_generation_proof = Some(signed.clone());
                }
                KeyshareState::ReadyForDecryption(rfd) => {
                    rfd.signed_pk_generation_proof = Some(signed.clone());
                }
                KeyshareState::Decrypting(d) => {
                    d.signed_pk_generation_proof = Some(signed.clone());
                }
                other => {
                    warn!(
                        "PkGenerationProofSigned in {:?} — C1 proof not stored (unexpected state)",
                        other.variant_name()
                    );
                }
            }
            Ok(s)
        })
    }

    fn keyshare_created_fields(
        state: &KeyshareState,
    ) -> Option<(&ArcBytes, &Option<SignedProofPayload>)> {
        use KeyshareState as K;
        match state {
            K::ReadyForDecryption(s) => Some((&s.pk_share, &s.signed_pk_generation_proof)),
            K::Decrypting(s) => Some((&s.pk_share, &s.signed_pk_generation_proof)),
            _ => None,
        }
    }

    fn try_finish_deferred_keyshare_publish(&mut self, ec: EventContext<Sequenced>) -> Result<()> {
        if !self.pending.keyshare_publish {
            return Ok(());
        }
        let state = self.state.try_get()?;
        let Some((_, signed)) = Self::keyshare_created_fields(&state.state) else {
            return Ok(());
        };
        if signed.is_none() {
            return Ok(());
        }
        self.pending.keyshare_publish = false;
        self.publish_keyshare_created(ec)
    }
}

impl Actor for ThresholdKeyshare {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
    }
}

#[path = "effects/mod.rs"]
mod effects;
#[path = "handlers.rs"]
mod handlers;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
