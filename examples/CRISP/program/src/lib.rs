// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use e3_compute_provider::FHEInputs;
use e3_fhe_params::decode_bfv_params_arc;
use fhe::bfv::Ciphertext;
use fhe_traits::{DeserializeParametrized, Serialize};

/// The input policy this E3 program requires.
///
/// Every E3 program exports one beside its processor, so the guest and the dev runner do not need
/// to know which program they are running.
pub fn policy() -> e3_compute_provider::InputPolicy {
    policy::crisp()
}

/// CRISP Implementation of the CiphertextProcessor function
pub fn fhe_processor(fhe_inputs: &FHEInputs) -> Vec<u8> {
    let params = decode_bfv_params_arc(&fhe_inputs.params).unwrap();

    let mut sum = Ciphertext::zero(&params);
    for ciphertext_bytes in &fhe_inputs.ciphertexts {
        let ciphertext = Ciphertext::from_bytes(&ciphertext_bytes.0, &params).unwrap();

        sum += &ciphertext;
    }

    sum.to_bytes()
}

/// CRISP's answers to how an input becomes a leaf and which inputs are tallied.
///
/// Both are specific to this program and its contract. They live here, beside the `CRISPProgram`
/// they must agree with, rather than in `e3-compute-provider`, which every E3 program shares.
pub mod policy {
    use e3_compute_provider::policy::{leaf_from_digest, PublishedInput};
    use e3_compute_provider::{ComputeError, InputPolicy};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;

    /// The 20-byte slot address `CRISPProgram` publishes with each input.
    fn slot_of(input: &PublishedInput) -> Result<[u8; 20], ComputeError> {
        <[u8; 20]>::try_from(input.metadata).map_err(|_| ComputeError::LeafCommitment {
            index: input.index,
            reason: format!(
                "expected a 20-byte slot address, got {} bytes",
                input.metadata.len()
            ),
        })
    }

    /// `sha256(sha256(ciphertext) || commitment || slot) mod SNARK_SCALAR_FIELD`.
    ///
    /// Must stay byte-identical to `CRISPProgram.inputLeaf`, or no root will ever match. It binds
    /// three things: the bytes, because the Noir proof constrains only the commitment and never
    /// sees the serialized ciphertext; the commitment, so no commitment can be paired with any
    /// ciphertext; and the slot, because selection is per slot and an unbound slot would let a
    /// prover re-group entries.
    pub fn leaf(input: &PublishedInput) -> Result<String, ComputeError> {
        let commitment = input
            .commitment
            .ok_or_else(|| ComputeError::LeafCommitment {
                index: input.index,
                reason: "CRISP publishes a commitment with every input".to_string(),
            })?;
        let slot = slot_of(input)?;

        let mut outer = Sha256::new();
        outer.update(Sha256::digest(input.ciphertext));
        outer.update(commitment);
        outer.update(slot);
        Ok(leaf_from_digest(&outer.finalize()))
    }

    /// The most recent entry per slot whose bytes reproduce its commitment.
    ///
    /// CRISP's input tree is append-only: anyone may write to any census member's slot, since the
    /// mask path checks no signature. Overwriting in place would let a third party replace the
    /// bytes of a counted vote and erase it. Appending keeps the earlier entry, and selecting the
    /// latest *usable* one falls back to it when a later entry contradicts its commitment.
    ///
    /// A slot whose entries are all unusable contributes nothing — it never held a good vote.
    pub fn latest_usable_per_slot(inputs: &[PublishedInput]) -> Vec<usize> {
        let mut latest: BTreeMap<[u8; 20], usize> = BTreeMap::new();
        for input in inputs {
            if !input.matches_commitment() {
                continue;
            }
            // Walking in order means a later entry replaces an earlier one for the same slot.
            if let Ok(slot) = slot_of(input) {
                latest.insert(slot, input.index);
            }
        }
        let mut selected: Vec<usize> = latest.into_values().collect();
        selected.sort_unstable();
        selected
    }

    /// The policy `CRISPProgram` requires.
    pub fn crisp() -> InputPolicy {
        InputPolicy {
            leaf,
            select: latest_usable_per_slot,
        }
    }
}
