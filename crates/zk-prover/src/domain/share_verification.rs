// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Pure, synchronous domain logic for C1/C2/C3/C4/C6 share-proof verification.
//!
//! The [`crate::actors::share_verification::ShareVerificationActor`] is a thin
//! transport shell: it owns the event bus and performs all publish/persist I/O.
//! This module owns the business logic — ECDSA validation, proof-commitment
//! hashing, consistency filtering, and ZK-result tallying — as pure functions on
//! the stateless [`ShareVerifier`] service, plus the per-E3 pending-state types.
//! It has NO actix / `BusHandle` / `Addr` concerns (tracing is allowed).

use std::collections::{BTreeSet, HashMap, HashSet};

use alloy::primitives::{keccak256, Address, Bytes};
use alloy::sol_types::SolValue;
use e3_events::{
    E3id, EventContext, PartyProofData, PartyProofsToVerify, PartyShareDecryptionProofsToVerify,
    PartyVerificationResult, ProofType, Sequenced, SignedProofPayload, VerificationKind,
};
use e3_utils::utility_types::ArcBytes;
use e3_zk_helpers::CiphernodesCommitteeSize;
use tracing::{info, warn};

/// Trait for party types whose signed proofs can be ECDSA-validated and ZK-verified.
pub(crate) trait VerifiableParty: Clone + PartialEq {
    fn party_id(&self) -> u64;
    fn signed_proofs(&self) -> Vec<SignedProofPayload>;
}

impl VerifiableParty for PartyProofsToVerify {
    fn party_id(&self) -> u64 {
        self.sender_party_id
    }
    fn signed_proofs(&self) -> Vec<SignedProofPayload> {
        self.signed_proofs.clone()
    }
}

impl VerifiableParty for PartyShareDecryptionProofsToVerify {
    fn party_id(&self) -> u64 {
        self.sender_party_id
    }
    fn signed_proofs(&self) -> Vec<SignedProofPayload> {
        std::iter::once(self.signed_sk_decryption_proof.clone())
            .chain(self.signed_e_sm_decryption_proofs.iter().cloned())
            .collect()
    }
}

/// ECDSA validation result for a single party.
pub(crate) struct EcdsaPartyResult {
    pub(crate) passed: bool,
    /// The pair (signed_payload, recovered_address) of the first failing proof, if any.
    pub(crate) failed_payload: Option<(SignedProofPayload, Option<Address>)>,
}

/// A single ECDSA failure to be attributed (emitted) by the actor.
pub(crate) struct EcdsaFailure {
    pub(crate) party_id: u64,
    pub(crate) signed: SignedProofPayload,
    pub(crate) recovered: Option<Address>,
}

/// Outcome of validating + preparing a batch of party proofs for the
/// consistency-check + ZK phases. Pure data; the actor performs the I/O.
pub(crate) struct EcdsaValidationOutcome<P> {
    pub(crate) ecdsa_dishonest: HashSet<u64>,
    /// Failures to emit, in party iteration order.
    pub(crate) failures: Vec<EcdsaFailure>,
    pub(crate) ecdsa_passed_parties: Vec<P>,
    pub(crate) party_addresses: HashMap<u64, Address>,
    pub(crate) party_proof_hashes: HashMap<u64, Vec<(ProofType, [u8; 32])>>,
    pub(crate) party_public_signals: HashMap<u64, Vec<(ProofType, ArcBytes)>>,
    pub(crate) party_proof_data: HashMap<u64, Vec<(ProofType, ArcBytes)>>,
    /// Assembled per-party data for the consistency-check request.
    pub(crate) consistency_party_data: Vec<PartyProofData>,
}

/// Pending verification state — stored while ZK verification is in flight.
pub(crate) struct PendingVerification {
    pub(crate) e3_id: E3id,
    pub(crate) kind: VerificationKind,
    pub(crate) ec: EventContext<Sequenced>,
    /// Parties that failed ECDSA (dishonest before ZK runs).
    pub(crate) ecdsa_dishonest: HashSet<u64>,
    /// Pre-dishonest parties from the dispatch (missing/incomplete proofs).
    pub(crate) pre_dishonest: BTreeSet<u64>,
    /// Party IDs dispatched for ZK verification (for cross-checking results).
    pub(crate) dispatched_party_ids: HashSet<u64>,
    /// Recovered address for each party (from ECDSA step).
    pub(crate) party_addresses: HashMap<u64, Address>,
    /// Cached (proof_type, data_hash) per party — for emitting ProofVerificationPassed.
    pub(crate) party_proof_hashes: HashMap<u64, Vec<(ProofType, [u8; 32])>>,
    /// Cached (proof_type, public_signals) per party — for commitment consistency checking.
    pub(crate) party_public_signals: HashMap<u64, Vec<(ProofType, ArcBytes)>>,
    /// Parallel to `party_public_signals` — raw `proof.data` per (party, proof_type).
    pub(crate) party_proof_data: HashMap<u64, Vec<(ProofType, ArcBytes)>>,
    /// BFV preset for circuit artifact resolution.
    #[allow(dead_code)]
    pub(crate) params_preset: e3_fhe_params::BfvPreset,
    /// Committee size for per-committee circuit artifact resolution.
    #[allow(dead_code)]
    pub(crate) committee_size: CiphernodesCommitteeSize,
}

/// Pending consistency check — stored between ECDSA pass and ZK dispatch.
pub(crate) struct PendingConsistencyCheck {
    pub(crate) e3_id: E3id,
    pub(crate) kind: VerificationKind,
    pub(crate) ec: EventContext<Sequenced>,
    /// Parties that failed ECDSA (dishonest before consistency runs).
    pub(crate) ecdsa_dishonest: HashSet<u64>,
    /// Pre-dishonest parties from the dispatch (missing/incomplete proofs).
    pub(crate) pre_dishonest: BTreeSet<u64>,
    /// Recovered address per ECDSA-passed party.
    pub(crate) party_addresses: HashMap<u64, Address>,
    /// (proof_type, data_hash) per party — for ProofVerificationPassed after ZK.
    pub(crate) party_proof_hashes: HashMap<u64, Vec<(ProofType, [u8; 32])>>,
    /// (proof_type, public_signals) per party — for consistency & ZK.
    pub(crate) party_public_signals: HashMap<u64, Vec<(ProofType, ArcBytes)>>,
    /// Parallel to `party_public_signals` — raw `proof.data` per (party, proof_type).
    pub(crate) party_proof_data: HashMap<u64, Vec<(ProofType, ArcBytes)>>,
    /// Original ECDSA-passed share proofs for ZK dispatch.
    pub(crate) ecdsa_passed_share_proofs: Vec<PartyProofsToVerify>,
    /// Original ECDSA-passed decryption proofs for ZK dispatch.
    pub(crate) ecdsa_passed_decryption_proofs: Vec<PartyShareDecryptionProofsToVerify>,
    /// BFV preset for circuit artifact resolution.
    pub(crate) params_preset: e3_fhe_params::BfvPreset,
    /// Committee size for per-committee circuit artifact resolution.
    pub(crate) committee_size: CiphernodesCommitteeSize,
}

/// Filter out inconsistent parties and collect dispatched party IDs.
/// Returns `None` if all parties were filtered out (nothing to verify).
pub(crate) fn filter_consistent<P>(
    proofs: Vec<P>,
    inconsistent: &BTreeSet<u64>,
    party_id_of: impl Fn(&P) -> u64,
) -> Option<(Vec<P>, HashSet<u64>)> {
    let passed: Vec<P> = proofs
        .into_iter()
        .filter(|p| !inconsistent.contains(&party_id_of(p)))
        .collect();
    if passed.is_empty() {
        return None;
    }
    let ids = passed.iter().map(party_id_of).collect();
    Some((passed, ids))
}

/// Per-party emission decision produced when tallying ZK verification results.
pub(crate) enum ZkPartyEmission {
    /// Party failed ZK — attribute fault using the signed payload.
    Failed {
        party_id: u64,
        signed: SignedProofPayload,
    },
    /// Party passed ZK — emit `ProofVerificationPassed` for each cached proof.
    Passed { party_id: u64 },
}

/// Outcome of tallying ZK verification results: the accumulated dishonest set
/// and the ordered emission decisions.
pub(crate) struct ZkTallyOutcome {
    pub(crate) dishonest: BTreeSet<u64>,
    pub(crate) emissions: Vec<ZkPartyEmission>,
}

/// Human-readable label for a verification kind (used in log lines).
pub(crate) fn label_for(kind: &VerificationKind) -> &'static str {
    match kind {
        VerificationKind::ShareProofs => "C2/C3",
        VerificationKind::ThresholdDecryptionProofs => "C6",
        VerificationKind::PkGenerationProofs => "C1",
        VerificationKind::DecryptionProofs => "C4",
    }
}

/// Stateless service holding all pure share-verification business logic.
pub(crate) struct ShareVerifier;

impl ShareVerifier {
    /// Keccak256 over `abi_encode((proof.data, proof.public_signals))`.
    fn proof_data_hash(signed: &SignedProofPayload) -> [u8; 32] {
        let msg = (
            Bytes::copy_from_slice(&signed.payload.proof.data),
            Bytes::copy_from_slice(&signed.payload.proof.public_signals),
        )
            .abi_encode();
        keccak256(&msg).into()
    }

    /// Check that a party supplied the canonical proof-type layout for this protocol phase.
    ///
    /// C2/C3 counts are derived from the DKG parameter preset. Variable C4b and C6 counts are
    /// checked against trusted local state by their producers. This trust-boundary check prevents
    /// a signed proof for another phase (or a duplicated singleton proof) from satisfying the
    /// current phase merely because its self-declared [`ProofType`] maps to a valid circuit.
    fn has_canonical_proof_shape(
        kind: &VerificationKind,
        signed_proofs: &[SignedProofPayload],
        params_preset: e3_fhe_params::BfvPreset,
    ) -> bool {
        match kind {
            VerificationKind::PkGenerationProofs => {
                signed_proofs.len() == 1
                    && signed_proofs[0].payload.proof_type == ProofType::C1PkGeneration
            }
            VerificationKind::ShareProofs => {
                // Canonical order is C2a, C2b, C3a x L, C3b x L. Share encryption uses the
                // DKG counterpart of a threshold preset (or the preset itself when already DKG).
                let dkg_preset = params_preset.dkg_counterpart().unwrap_or(params_preset);
                let num_moduli = dkg_preset.metadata().num_moduli;
                signed_proofs.len() == 2 + (2 * num_moduli)
                    && signed_proofs[0].payload.proof_type == ProofType::C2aSkShareComputation
                    && signed_proofs[1].payload.proof_type == ProofType::C2bESmShareComputation
                    && signed_proofs[2..2 + num_moduli]
                        .iter()
                        .all(|signed| signed.payload.proof_type == ProofType::C3aSkShareEncryption)
                    && signed_proofs[2 + num_moduli..]
                        .iter()
                        .all(|signed| signed.payload.proof_type == ProofType::C3bESmShareEncryption)
            }
            VerificationKind::DecryptionProofs => {
                // PartyShareDecryptionProofsToVerify has one distinguished C4a slot followed
                // by one or more C4b slots. The producer checks the exact C4b count against
                // `es_poly_sum`; here we bind every signed payload to its structural role because
                // C4a/C4b share a CircuitName.
                signed_proofs.len() >= 2
                    && signed_proofs[0].payload.proof_type == ProofType::C4aSkShareDecryption
                    && signed_proofs[1..]
                        .iter()
                        .all(|signed| signed.payload.proof_type == ProofType::C4bESmShareDecryption)
            }
            VerificationKind::ThresholdDecryptionProofs => {
                !signed_proofs.is_empty()
                    && signed_proofs.iter().all(|signed| {
                        signed.payload.proof_type == ProofType::C6ThresholdShareDecryption
                    })
            }
        }
    }

    /// Validate ECDSA properties for a set of signed proofs from one party:
    /// 1. e3_id match
    /// 2. Signature recovery (valid ECDSA)
    /// 3. Recovered signer owns the canonical finalized-committee party slot
    /// 4. Signer consistency (all proofs from same address)
    /// 5. Circuit name matches expected ProofType circuits
    pub(crate) fn ecdsa_validate_signed_proofs(
        sender_party_id: u64,
        signed_proofs: &[SignedProofPayload],
        e3_id_str: &str,
        label: &str,
        expected_signer: Option<Address>,
    ) -> EcdsaPartyResult {
        if signed_proofs.is_empty() {
            info!(
                "{} party {} supplied an empty signed-proof bundle",
                label, sender_party_id
            );
            return EcdsaPartyResult {
                passed: false,
                failed_payload: None,
            };
        }

        let Some(expected_signer) = expected_signer else {
            info!(
                "{} party {} has no canonical finalized-committee slot",
                label, sender_party_id
            );
            return EcdsaPartyResult {
                passed: false,
                // The outer party id is not part of the signed payload. Its absence from the
                // canonical committee is therefore a structural dispatch failure, not
                // self-authenticating evidence that can safely be attributed to the signer.
                failed_payload: None,
            };
        };

        let mut expected_addr: Option<Address> = None;

        for signed in signed_proofs {
            // 1. e3_id match
            if signed.payload.e3_id.to_string() != e3_id_str {
                info!(
                    "{} proof from party {} has wrong e3_id ({} vs {})",
                    label, sender_party_id, signed.payload.e3_id, e3_id_str
                );
                return EcdsaPartyResult {
                    passed: false,
                    failed_payload: Some((signed.clone(), expected_addr)),
                };
            }

            // 2. Signature recovery
            match signed.recover_address() {
                Ok(addr) => {
                    // 3. Canonical party ownership and signer consistency
                    if addr != expected_signer {
                        info!(
                            "{} proof signer {} does not own party {} (expected {})",
                            label, addr, sender_party_id, expected_signer
                        );
                        return EcdsaPartyResult {
                            passed: false,
                            failed_payload: Some((signed.clone(), Some(addr))),
                        };
                    }
                    match &expected_addr {
                        Some(ea) if *ea != addr => {
                            info!(
                                "{} inconsistent signer for party {}",
                                label, sender_party_id
                            );
                            return EcdsaPartyResult {
                                passed: false,
                                failed_payload: Some((signed.clone(), Some(addr))),
                            };
                        }
                        None => expected_addr = Some(addr),
                        _ => {}
                    }
                }
                Err(e) => {
                    info!(
                        "{} signature recovery failed for party {} ({:?}): {}",
                        label, sender_party_id, signed.payload.proof_type, e
                    );
                    return EcdsaPartyResult {
                        passed: false,
                        failed_payload: Some((signed.clone(), expected_addr)),
                    };
                }
            }

            // 4. Circuit name validation
            let expected_circuits = signed.payload.proof_type.circuit_names();
            if !expected_circuits.contains(&signed.payload.proof.circuit) {
                info!(
                    "{} circuit mismatch for party {}: expected {:?}, got {:?}",
                    label, sender_party_id, expected_circuits, signed.payload.proof.circuit
                );
                return EcdsaPartyResult {
                    passed: false,
                    failed_payload: Some((signed.clone(), expected_addr)),
                };
            }
        }

        EcdsaPartyResult {
            passed: true,
            failed_payload: None,
        }
    }

    /// Run ECDSA validation across all parties and prepare the cached proof
    /// hashes/signals/data plus the consistency-check request payload for the
    /// parties that passed. Pure: no event publishing.
    pub(crate) fn validate_and_prepare<P: VerifiableParty>(
        party_proofs: &[P],
        e3_id_str: &str,
        kind: &VerificationKind,
        label: &str,
        committee: Option<&[Address]>,
        params_preset: e3_fhe_params::BfvPreset,
        committee_size: CiphernodesCommitteeSize,
    ) -> EcdsaValidationOutcome<P> {
        let mut ecdsa_dishonest = HashSet::new();
        let mut failures = Vec::new();
        let mut ecdsa_passed_parties = Vec::new();
        let mut party_addresses: HashMap<u64, Address> = HashMap::new();

        // A finalized committee is a one-to-one party-slot map. The on-chain sortition path
        // guarantees unique operators, but replayed/test-produced events also cross this
        // boundary. Refuse an ambiguous map rather than allowing one signer to own two slots.
        let expected_committee_n = committee_size.values().n;
        let committee = committee.filter(|members| {
            let unique_members: HashSet<Address> = members.iter().copied().collect();
            let is_unique = unique_members.len() == members.len();
            let has_expected_size = members.len() == expected_committee_n;
            if !is_unique {
                warn!(
                    "{} finalized committee contains duplicate addresses; rejecting proof batch",
                    label
                );
            }
            if !has_expected_size {
                warn!(
                    "{} finalized committee has {} members, expected {}; rejecting proof batch",
                    label,
                    members.len(),
                    expected_committee_n
                );
            }
            is_unique && has_expected_size
        });

        // Collapse byte-identical replayed bundles idempotently. Two different bundles for the
        // same outer party id are ambiguous and are both excluded, so a canonical party can
        // contribute at most one verification result and one set of passed-proof emissions.
        let mut unique_parties: Vec<&P> = Vec::new();
        let mut party_positions: HashMap<u64, usize> = HashMap::new();
        let mut conflicting_party_ids = HashSet::new();
        for party in party_proofs {
            match party_positions.get(&party.party_id()).copied() {
                Some(position) if unique_parties[position] == party => {
                    warn!(
                        "{} duplicate replay for party {} ignored",
                        label,
                        party.party_id()
                    );
                }
                Some(_) => {
                    warn!(
                        "{} conflicting proof bundles for party {}; rejecting that party",
                        label,
                        party.party_id()
                    );
                    conflicting_party_ids.insert(party.party_id());
                }
                None => {
                    party_positions.insert(party.party_id(), unique_parties.len());
                    unique_parties.push(party);
                }
            }
        }

        for party in unique_parties {
            if conflicting_party_ids.contains(&party.party_id()) {
                ecdsa_dishonest.insert(party.party_id());
                continue;
            }

            let proofs = party.signed_proofs();
            if !Self::has_canonical_proof_shape(kind, &proofs, params_preset) {
                info!(
                    "{} party {} supplied a non-canonical proof-type multiplicity/order",
                    label,
                    party.party_id()
                );
                // The verification kind and outer bundle are not signed. Exclude the party,
                // but do not manufacture slash evidence from an otherwise valid signed proof.
                ecdsa_dishonest.insert(party.party_id());
                continue;
            }

            let expected_signer = usize::try_from(party.party_id())
                .ok()
                .and_then(|party_id| committee.and_then(|members| members.get(party_id)))
                .copied();
            let result = Self::ecdsa_validate_signed_proofs(
                party.party_id(),
                &proofs,
                e3_id_str,
                label,
                expected_signer,
            );
            if result.passed {
                ecdsa_passed_parties.push(party.clone());
            } else {
                ecdsa_dishonest.insert(party.party_id());
                if let Some((signed, addr)) = result.failed_payload {
                    failures.push(EcdsaFailure {
                        party_id: party.party_id(),
                        signed,
                        recovered: addr,
                    });
                }
            }
        }

        // Store recovered addresses for passed parties.
        for party in &ecdsa_passed_parties {
            let proofs = party.signed_proofs();
            if let Some(first_signed) = proofs.first() {
                if let Ok(addr) = first_signed.recover_address() {
                    party_addresses.insert(party.party_id(), addr);
                }
            }
        }

        // Compute proof hashes and public signals for ECDSA-passed parties.
        let mut party_proof_hashes: HashMap<u64, Vec<(ProofType, [u8; 32])>> = HashMap::new();
        let mut party_public_signals: HashMap<u64, Vec<(ProofType, ArcBytes)>> = HashMap::new();
        let mut party_raw_proof_data: HashMap<u64, Vec<(ProofType, ArcBytes)>> = HashMap::new();
        for party in &ecdsa_passed_parties {
            let hashes: Vec<(ProofType, [u8; 32])> = party
                .signed_proofs()
                .iter()
                .map(|signed| (signed.payload.proof_type, Self::proof_data_hash(signed)))
                .collect();
            let signals: Vec<(ProofType, ArcBytes)> = party
                .signed_proofs()
                .iter()
                .map(|signed| {
                    (
                        signed.payload.proof_type,
                        signed.payload.proof.public_signals.clone(),
                    )
                })
                .collect();
            let datas: Vec<(ProofType, ArcBytes)> = party
                .signed_proofs()
                .iter()
                .map(|signed| (signed.payload.proof_type, signed.payload.proof.data.clone()))
                .collect();
            party_proof_hashes.insert(party.party_id(), hashes);
            party_public_signals.insert(party.party_id(), signals);
            party_raw_proof_data.insert(party.party_id(), datas);
        }

        // Assemble consistency-check request payload.
        let consistency_party_data: Vec<PartyProofData> = ecdsa_passed_parties
            .iter()
            .map(|party| {
                let signals = party_public_signals
                    .get(&party.party_id())
                    .cloned()
                    .unwrap_or_default();
                let hashes = party_proof_hashes
                    .get(&party.party_id())
                    .cloned()
                    .unwrap_or_default();
                let raw_datas = party_raw_proof_data
                    .get(&party.party_id())
                    .cloned()
                    .unwrap_or_default();
                let proofs = signals
                    .into_iter()
                    .zip(hashes)
                    .zip(raw_datas)
                    .map(|(((pt, ps), (_, dh)), (_, pd))| (pt, ps, dh, pd))
                    .collect();
                PartyProofData {
                    party_id: party.party_id(),
                    address: party_addresses
                        .get(&party.party_id())
                        .copied()
                        .unwrap_or_default(),
                    proofs,
                }
            })
            .collect();

        EcdsaValidationOutcome {
            ecdsa_dishonest,
            failures,
            ecdsa_passed_parties,
            party_addresses,
            party_proof_hashes,
            party_public_signals,
            party_proof_data: party_raw_proof_data,
            consistency_party_data,
        }
    }

    /// Tally ZK verification results against the dispatched set: accumulate the
    /// dishonest party set (including parties missing from the response) and
    /// produce ordered per-party emission decisions. Pure: no event publishing.
    pub(crate) fn tally_zk_results(
        pre_dishonest: BTreeSet<u64>,
        ecdsa_dishonest: &HashSet<u64>,
        dispatched_party_ids: &HashSet<u64>,
        zk_results: &[PartyVerificationResult],
    ) -> ZkTallyOutcome {
        let mut dishonest: BTreeSet<u64> = pre_dishonest;
        dishonest.extend(ecdsa_dishonest);

        // Canonicalize the worker response before tallying. Exact duplicate responses are
        // harmless replays and are collapsed; conflicting responses for one party fail closed.
        // This also guarantees at most one Passed/Failed emission per canonical party.
        let mut unique_results: HashMap<u64, &PartyVerificationResult> = HashMap::new();
        let mut result_order = Vec::new();
        let mut conflicting_party_ids = HashSet::new();
        for result in zk_results {
            if !dispatched_party_ids.contains(&result.sender_party_id) {
                warn!(
                    "ZK result for party {} was not dispatched — ignoring",
                    result.sender_party_id
                );
                continue;
            }

            match unique_results.get(&result.sender_party_id).copied() {
                Some(existing) if existing == result => {
                    warn!(
                        "Duplicate ZK result replay for party {} ignored",
                        result.sender_party_id
                    );
                }
                Some(_) => {
                    warn!(
                        "Conflicting ZK results for party {} — treating as dishonest",
                        result.sender_party_id
                    );
                    conflicting_party_ids.insert(result.sender_party_id);
                }
                None => {
                    unique_results.insert(result.sender_party_id, result);
                    result_order.push(result.sender_party_id);
                }
            }
        }

        // Cross-check: every dispatched party must appear exactly once after replay collapse.
        for &dispatched_pid in dispatched_party_ids {
            if !unique_results.contains_key(&dispatched_pid) {
                warn!(
                    "Party {} was dispatched for ZK verification but missing from results — treating as dishonest",
                    dispatched_pid
                );
                dishonest.insert(dispatched_pid);
            }
        }

        let mut emissions = Vec::new();
        for party_id in result_order {
            if conflicting_party_ids.contains(&party_id) {
                dishonest.insert(party_id);
                continue;
            }
            let Some(result) = unique_results.get(&party_id) else {
                // `result_order` and `unique_results` are populated together above. Keep this
                // boundary panic-free if that implementation changes later.
                dishonest.insert(party_id);
                continue;
            };
            if !result.all_verified {
                dishonest.insert(result.sender_party_id);
                if let Some(ref signed) = result.failed_signed_payload {
                    emissions.push(ZkPartyEmission::Failed {
                        party_id: result.sender_party_id,
                        signed: signed.clone(),
                    });
                }
            } else {
                emissions.push(ZkPartyEmission::Passed {
                    party_id: result.sender_party_id,
                });
            }
        }

        ZkTallyOutcome {
            dishonest,
            emissions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::local::PrivateKeySigner;
    use e3_events::{Proof, ProofPayload, ProofType};
    use e3_fhe_params::BfvPreset;

    fn signer() -> PrivateKeySigner {
        PrivateKeySigner::random()
    }

    fn minimum_committee(mut members: Vec<Address>) -> Vec<Address> {
        while members.len() < CiphernodesCommitteeSize::Minimum.values().n {
            let candidate = signer().address();
            if !members.contains(&candidate) {
                members.push(candidate);
            }
        }
        members
    }

    fn signed_proof(
        s: &PrivateKeySigner,
        e3_id: &E3id,
        proof_type: ProofType,
        marker: u8,
    ) -> SignedProofPayload {
        let proof = Proof::new(
            proof_type.circuit_names()[0],
            ArcBytes::from_bytes(&[marker, 2, 3]),
            ArcBytes::from_bytes(&[4, 5, marker]),
        );
        let payload = ProofPayload {
            e3_id: e3_id.clone(),
            proof_type,
            proof,
        };
        SignedProofPayload::sign(payload, s).expect("sign")
    }

    /// Build a signed C1 (PkGeneration) proof for `party_id` under `e3_id`,
    /// optionally with a deliberately wrong circuit name.
    fn signed_pk(s: &PrivateKeySigner, e3_id: &E3id, wrong_circuit: bool) -> SignedProofPayload {
        use e3_events::CircuitName;
        let proof_type = ProofType::C1PkGeneration;
        if !wrong_circuit {
            return signed_proof(s, e3_id, proof_type, 1);
        }
        let circuit = if wrong_circuit {
            CircuitName::PkBfv
        } else {
            proof_type.circuit_names()[0]
        };
        let proof = Proof::new(
            circuit,
            ArcBytes::from_bytes(&[1, 2, 3]),
            ArcBytes::from_bytes(&[4, 5, 6]),
        );
        let payload = ProofPayload {
            e3_id: e3_id.clone(),
            proof_type,
            proof,
        };
        SignedProofPayload::sign(payload, s).expect("sign")
    }

    fn e3() -> E3id {
        E3id::new("1", 1)
    }

    #[test]
    fn ecdsa_passes_for_well_formed_proof() {
        let s = signer();
        let e3 = e3();
        let p = signed_pk(&s, &e3, false);
        let res = ShareVerifier::ecdsa_validate_signed_proofs(
            7,
            &[p],
            &e3.to_string(),
            "C1",
            Some(s.address()),
        );
        assert!(res.passed);
        assert!(res.failed_payload.is_none());
    }

    #[test]
    fn ecdsa_fails_on_wrong_e3_id() {
        let s = signer();
        let p = signed_pk(&s, &e3(), false);
        let res =
            ShareVerifier::ecdsa_validate_signed_proofs(7, &[p], "999/0", "C1", Some(s.address()));
        assert!(!res.passed);
        assert!(res.failed_payload.is_some());
    }

    #[test]
    fn ecdsa_fails_on_circuit_mismatch() {
        let s = signer();
        let e3 = e3();
        let p = signed_pk(&s, &e3, true);
        let res = ShareVerifier::ecdsa_validate_signed_proofs(
            7,
            &[p],
            &e3.to_string(),
            "C1",
            Some(s.address()),
        );
        assert!(!res.passed);
    }

    #[test]
    fn ecdsa_fails_on_inconsistent_signer() {
        let s1 = signer();
        let s2 = signer();
        let e3 = e3();
        let p1 = signed_pk(&s1, &e3, false);
        let p2 = signed_pk(&s2, &e3, false);
        let res = ShareVerifier::ecdsa_validate_signed_proofs(
            7,
            &[p1, p2],
            &e3.to_string(),
            "C1",
            Some(s1.address()),
        );
        assert!(!res.passed);
    }

    #[test]
    fn ecdsa_fails_when_signer_does_not_own_party_slot() {
        let proof_signer = signer();
        let slot_owner = signer();
        let e3 = e3();
        let proof = signed_pk(&proof_signer, &e3, false);

        let result = ShareVerifier::ecdsa_validate_signed_proofs(
            1,
            &[proof],
            &e3.to_string(),
            "C1",
            Some(slot_owner.address()),
        );

        assert!(!result.passed);
        let (_, recovered) = result.failed_payload.expect("attributable mismatch");
        assert_eq!(recovered, Some(proof_signer.address()));
    }

    #[test]
    fn ecdsa_fails_for_empty_bundle_or_missing_party_slot() {
        let e3 = e3();
        let owner = signer();
        let empty = ShareVerifier::ecdsa_validate_signed_proofs(
            0,
            &[],
            &e3.to_string(),
            "C1",
            Some(owner.address()),
        );
        assert!(!empty.passed);
        assert!(empty.failed_payload.is_none());

        let proof = signed_pk(&owner, &e3, false);
        let missing =
            ShareVerifier::ecdsa_validate_signed_proofs(3, &[proof], &e3.to_string(), "C1", None);
        assert!(!missing.passed);
    }

    #[test]
    fn prepare_rejects_one_signer_relabelled_across_other_party_slots() {
        let first = signer();
        let second = signer();
        let third = signer();
        let e3 = e3();
        let parties = vec![
            PartyProofsToVerify {
                sender_party_id: 0,
                signed_proofs: vec![signed_pk(&first, &e3, false)],
            },
            PartyProofsToVerify {
                sender_party_id: 1,
                // A valid proof from party 0 cannot fill party 1's slot.
                signed_proofs: vec![signed_pk(&first, &e3, false)],
            },
            PartyProofsToVerify {
                sender_party_id: 2,
                // Nor can the same signer fill any other canonical slot.
                signed_proofs: vec![signed_pk(&first, &e3, false)],
            },
        ];
        let committee = [first.address(), second.address(), third.address()];

        let outcome = ShareVerifier::validate_and_prepare(
            &parties,
            &e3.to_string(),
            &VerificationKind::PkGenerationProofs,
            "C1",
            Some(&committee),
            BfvPreset::InsecureDkg512,
            CiphernodesCommitteeSize::Minimum,
        );

        assert_eq!(outcome.ecdsa_passed_parties.len(), 1);
        assert_eq!(outcome.ecdsa_passed_parties[0].sender_party_id, 0);
        assert_eq!(outcome.ecdsa_dishonest, HashSet::from([1, 2]));
    }

    #[test]
    fn canonical_shape_rejects_cross_phase_and_singleton_multiplicity() {
        let s = signer();
        let e3 = e3();

        let c1 = signed_proof(&s, &e3, ProofType::C1PkGeneration, 1);
        assert!(ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::PkGenerationProofs,
            std::slice::from_ref(&c1),
            BfvPreset::InsecureDkg512,
        ));
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::PkGenerationProofs,
            &[c1.clone(), c1.clone()],
            BfvPreset::InsecureDkg512,
        ));
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ThresholdDecryptionProofs,
            std::slice::from_ref(&c1),
            BfvPreset::InsecureDkg512,
        ));
        let c6 = signed_proof(&s, &e3, ProofType::C6ThresholdShareDecryption, 9);
        assert!(ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ThresholdDecryptionProofs,
            std::slice::from_ref(&c6),
            BfvPreset::InsecureDkg512,
        ));
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ThresholdDecryptionProofs,
            &[],
            BfvPreset::InsecureDkg512,
        ));

        let share_bundle = vec![
            signed_proof(&s, &e3, ProofType::C2aSkShareComputation, 2),
            signed_proof(&s, &e3, ProofType::C2bESmShareComputation, 3),
            signed_proof(&s, &e3, ProofType::C3aSkShareEncryption, 4),
            signed_proof(&s, &e3, ProofType::C3bESmShareEncryption, 5),
        ];
        assert!(ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ShareProofs,
            &share_bundle,
            BfvPreset::InsecureDkg512,
        ));
        let mut duplicate_c2a = share_bundle.clone();
        duplicate_c2a.insert(1, duplicate_c2a[0].clone());
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ShareProofs,
            &duplicate_c2a,
            BfvPreset::InsecureDkg512,
        ));

        let secure_share_bundle = vec![
            signed_proof(&s, &e3, ProofType::C2aSkShareComputation, 10),
            signed_proof(&s, &e3, ProofType::C2bESmShareComputation, 11),
            signed_proof(&s, &e3, ProofType::C3aSkShareEncryption, 12),
            signed_proof(&s, &e3, ProofType::C3aSkShareEncryption, 13),
            signed_proof(&s, &e3, ProofType::C3bESmShareEncryption, 14),
            signed_proof(&s, &e3, ProofType::C3bESmShareEncryption, 15),
        ];
        assert!(ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ShareProofs,
            &secure_share_bundle,
            BfvPreset::SecureThreshold8192,
        ));
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::ShareProofs,
            &share_bundle,
            BfvPreset::SecureThreshold8192,
        ));

        let c4_bundle = vec![
            signed_proof(&s, &e3, ProofType::C4aSkShareDecryption, 6),
            signed_proof(&s, &e3, ProofType::C4bESmShareDecryption, 7),
        ];
        assert!(ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::DecryptionProofs,
            &c4_bundle,
            BfvPreset::InsecureDkg512,
        ));
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::DecryptionProofs,
            &c4_bundle[1..],
            BfvPreset::InsecureDkg512,
        ));
        let mut extra_c4b = c4_bundle.clone();
        extra_c4b.push(c4_bundle[1].clone());
        assert!(ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::DecryptionProofs,
            &extra_c4b,
            BfvPreset::InsecureDkg512,
        ));
        let mut wrong_c4_tail = c4_bundle.clone();
        wrong_c4_tail.push(c6);
        assert!(!ShareVerifier::has_canonical_proof_shape(
            &VerificationKind::DecryptionProofs,
            &wrong_c4_tail,
            BfvPreset::InsecureDkg512,
        ));
    }

    #[test]
    fn prepare_excludes_wrong_phase_without_creating_slash_evidence() {
        let s = signer();
        let e3 = e3();
        let parties = [PartyProofsToVerify {
            sender_party_id: 0,
            signed_proofs: vec![signed_proof(
                &s,
                &e3,
                ProofType::C6ThresholdShareDecryption,
                9,
            )],
        }];
        let committee = minimum_committee(vec![s.address()]);

        let outcome = ShareVerifier::validate_and_prepare(
            &parties,
            &e3.to_string(),
            &VerificationKind::PkGenerationProofs,
            "C1",
            Some(&committee),
            BfvPreset::InsecureDkg512,
            CiphernodesCommitteeSize::Minimum,
        );

        assert!(outcome.ecdsa_passed_parties.is_empty());
        assert_eq!(outcome.ecdsa_dishonest, HashSet::from([0]));
        assert!(outcome.failures.is_empty());
    }

    #[test]
    fn prepare_collapses_identical_party_replay_and_rejects_conflict() {
        let s = signer();
        let e3 = e3();
        let committee = minimum_committee(vec![s.address()]);
        let party = PartyProofsToVerify {
            sender_party_id: 0,
            signed_proofs: vec![signed_proof(&s, &e3, ProofType::C1PkGeneration, 1)],
        };

        let replayed = ShareVerifier::validate_and_prepare(
            &[party.clone(), party.clone()],
            &e3.to_string(),
            &VerificationKind::PkGenerationProofs,
            "C1",
            Some(&committee),
            BfvPreset::InsecureDkg512,
            CiphernodesCommitteeSize::Minimum,
        );
        assert_eq!(replayed.ecdsa_passed_parties, vec![party.clone()]);
        assert!(replayed.ecdsa_dishonest.is_empty());
        assert_eq!(replayed.consistency_party_data.len(), 1);

        let conflicting = PartyProofsToVerify {
            sender_party_id: 0,
            signed_proofs: vec![signed_proof(&s, &e3, ProofType::C1PkGeneration, 2)],
        };
        let conflict = ShareVerifier::validate_and_prepare(
            &[party, conflicting],
            &e3.to_string(),
            &VerificationKind::PkGenerationProofs,
            "C1",
            Some(&committee),
            BfvPreset::InsecureDkg512,
            CiphernodesCommitteeSize::Minimum,
        );
        assert!(conflict.ecdsa_passed_parties.is_empty());
        assert_eq!(conflict.ecdsa_dishonest, HashSet::from([0]));
        assert!(conflict.failures.is_empty());
    }

    #[test]
    fn prepare_rejects_ambiguous_committee_where_one_signer_owns_multiple_slots() {
        let s = signer();
        let e3 = e3();
        let parties = vec![
            PartyProofsToVerify {
                sender_party_id: 0,
                signed_proofs: vec![signed_proof(&s, &e3, ProofType::C1PkGeneration, 1)],
            },
            PartyProofsToVerify {
                sender_party_id: 1,
                signed_proofs: vec![signed_proof(&s, &e3, ProofType::C1PkGeneration, 2)],
            },
        ];
        let ambiguous_committee = [s.address(), s.address(), signer().address()];

        let outcome = ShareVerifier::validate_and_prepare(
            &parties,
            &e3.to_string(),
            &VerificationKind::PkGenerationProofs,
            "C1",
            Some(&ambiguous_committee),
            BfvPreset::InsecureDkg512,
            CiphernodesCommitteeSize::Minimum,
        );

        assert!(outcome.ecdsa_passed_parties.is_empty());
        assert_eq!(outcome.ecdsa_dishonest, HashSet::from([0, 1]));
        assert!(outcome.failures.is_empty());
    }

    #[test]
    fn prepare_rejects_committee_with_wrong_circuit_dimension() {
        let s = signer();
        let e3 = e3();
        let parties = [PartyProofsToVerify {
            sender_party_id: 0,
            signed_proofs: vec![signed_proof(&s, &e3, ProofType::C1PkGeneration, 1)],
        }];
        let undersized_committee = [s.address()];

        let outcome = ShareVerifier::validate_and_prepare(
            &parties,
            &e3.to_string(),
            &VerificationKind::PkGenerationProofs,
            "C1",
            Some(&undersized_committee),
            BfvPreset::InsecureDkg512,
            CiphernodesCommitteeSize::Minimum,
        );

        assert!(outcome.ecdsa_passed_parties.is_empty());
        assert_eq!(outcome.ecdsa_dishonest, HashSet::from([0]));
        assert!(outcome.failures.is_empty());
    }

    #[test]
    fn filter_consistent_drops_inconsistent_and_returns_ids() {
        let proofs = vec![1u64, 2, 3];
        let inconsistent: BTreeSet<u64> = [2].into_iter().collect();
        let (passed, ids) = filter_consistent(proofs, &inconsistent, |p| *p).expect("some");
        assert_eq!(passed, vec![1, 3]);
        assert!(ids.contains(&1) && ids.contains(&3) && !ids.contains(&2));
    }

    #[test]
    fn filter_consistent_returns_none_when_all_filtered() {
        let proofs = vec![1u64, 2];
        let inconsistent: BTreeSet<u64> = [1, 2].into_iter().collect();
        assert!(filter_consistent(proofs, &inconsistent, |p| *p).is_none());
    }

    #[test]
    fn tally_marks_missing_dispatched_party_dishonest() {
        let dispatched: HashSet<u64> = [1, 2].into_iter().collect();
        let ecdsa: HashSet<u64> = HashSet::new();
        // No ZK results at all → both dispatched parties are missing → dishonest.
        let out = ShareVerifier::tally_zk_results(BTreeSet::new(), &ecdsa, &dispatched, &[]);
        assert!(out.dishonest.contains(&1));
        assert!(out.dishonest.contains(&2));
        assert!(out.emissions.is_empty());
    }

    #[test]
    fn tally_collapses_identical_result_replay() {
        let dispatched = HashSet::from([1]);
        let result = PartyVerificationResult {
            sender_party_id: 1,
            all_verified: true,
            failed_signed_payload: None,
            recovered_address: None,
        };

        let out = ShareVerifier::tally_zk_results(
            BTreeSet::new(),
            &HashSet::new(),
            &dispatched,
            &[result.clone(), result],
        );

        assert!(out.dishonest.is_empty());
        assert_eq!(out.emissions.len(), 1);
        assert!(matches!(
            out.emissions[0],
            ZkPartyEmission::Passed { party_id: 1 }
        ));
    }

    #[test]
    fn tally_rejects_conflicting_results_for_one_party() {
        let dispatched = HashSet::from([1]);
        let passed = PartyVerificationResult {
            sender_party_id: 1,
            all_verified: true,
            failed_signed_payload: None,
            recovered_address: None,
        };
        let failed = PartyVerificationResult {
            all_verified: false,
            ..passed.clone()
        };

        let out = ShareVerifier::tally_zk_results(
            BTreeSet::new(),
            &HashSet::new(),
            &dispatched,
            &[passed, failed],
        );

        assert_eq!(out.dishonest, BTreeSet::from([1]));
        assert!(out.emissions.is_empty());
    }
}
