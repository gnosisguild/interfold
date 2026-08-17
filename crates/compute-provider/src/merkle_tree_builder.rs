// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::compute_input::{ComputeError, FHEInputs};
use ark_bn254::Fr;
use ark_ff::{BigInt, BigInteger};
use e3_bfv_client::client::compute_ct_commitment;
use fhe::bfv::BfvParameters;
use light_poseidon::{Poseidon, PoseidonHasher};
use num_bigint::BigUint;
use num_traits::Num;
use sha2::{Digest as Sha2Digest, Sha256};
use std::collections::BTreeMap;

/// One input, paired with its position in the tree so the selection stays ordered.
type PositionedInput = (usize, (Vec<u8>, u64));
use std::str::FromStr;
use zk_kit_imt::imt::IMT;

/// The BN254 scalar field, which every tree leaf must be reduced into.
const SNARK_SCALAR_FIELD: &str =
    "21888242871839275222246405745257275088548364400416034343698204186575808495617";

pub struct MerkleTreeBuilder {
    pub leaf_hashes: Vec<String>,
    pub arity: usize,
    pub zero_value: String,
    pub depth: usize,
}

impl MerkleTreeBuilder {
    pub fn new(num_leaves: usize) -> Self {
        Self {
            leaf_hashes: Vec::new(),
            arity: 2,
            zero_value: "0".to_string(),
            depth: ((num_leaves as f64).log2().ceil() as usize).max(1),
        }
    }

    /// Sets the leaves directly, for tests that need a known tree.
    ///
    /// Never use this to build a tree the journal publishes. A Secure Process must derive its
    /// leaves from the ciphertexts it consumed, with [`Self::compute_leaf_hashes`]. Leaves that
    /// arrive as a separate value can disagree with those ciphertexts.
    #[cfg(test)]
    pub fn with_leaf_hashes(mut self, leaf_hashes: Vec<String>) -> Self {
        self.leaf_hashes = leaf_hashes;
        self
    }

    /// Derives one leaf per input, in the order given, and returns the inputs to compute over.
    ///
    /// The leaf matches the layout the E3 program builds:
    /// `sha256(sha256(bytes) || commitment || slot) mod SNARK_SCALAR_FIELD`.
    ///
    /// The tree is append-only, so a slot may hold several entries. For each slot the most recent
    /// entry whose bytes reproduce its commitment is returned; entries that do not reproduce it,
    /// or that do not deserialize, keep their leaf but are skipped. That fallback is what stops a
    /// third party appending bad bytes to a slot and erasing a vote that was already counted.
    ///
    /// The selection is a function of values the root binds, so any prover holding the same
    /// published data reaches the same set.
    ///
    /// When `commitments` or `slots` is empty, no check is possible: every input is returned and
    /// the leaf falls back to the bare ciphertext commitment.
    pub fn compute_leaf_hashes(
        &mut self,
        inputs: &FHEInputs,
        params: &BfvParameters,
    ) -> Result<Vec<(Vec<u8>, u64)>, ComputeError> {
        let degree = params.degree();
        let plaintext_modulus = params.plaintext();
        let moduli = params.moduli().to_vec();
        let bound = !inputs.commitments.is_empty() && !inputs.slots.is_empty();

        if bound
            && (inputs.commitments.len() != inputs.ciphertexts.len()
                || inputs.slots.len() != inputs.ciphertexts.len())
        {
            return Err(ComputeError::MerkleTree(format!(
                "{} ciphertexts, {} commitments, {} slots",
                inputs.ciphertexts.len(),
                inputs.commitments.len(),
                inputs.slots.len()
            )));
        }

        // Keyed by slot; a later entry replaces an earlier one, so what survives is the most
        // recent usable entry for that slot.
        let mut latest_per_slot: BTreeMap<[u8; 20], PositionedInput> = BTreeMap::new();
        let mut unbound_usable = Vec::new();

        for (index, item) in inputs.ciphertexts.iter().enumerate() {
            // A commitment that cannot be computed marks an unusable input rather than a failure:
            // the bytes are attacker-supplied and must not be able to stop the round.
            let recomputed =
                compute_ct_commitment(item.0.clone(), degree, plaintext_modulus, moduli.clone())
                    .ok();

            if !bound {
                let commitment = recomputed.ok_or(ComputeError::LeafCommitment {
                    index,
                    reason: "ciphertext could not be deserialized".to_string(),
                })?;
                self.leaf_hashes.push(hex::encode(commitment));
                unbound_usable.push(item.clone());
                continue;
            }

            self.leaf_hashes.push(Self::input_leaf(
                &item.0,
                &inputs.commitments[index],
                &inputs.slots[index],
            ));

            if recomputed == Some(inputs.commitments[index]) {
                latest_per_slot.insert(inputs.slots[index], (index, item.clone()));
            }
        }

        if !bound {
            return Ok(unbound_usable);
        }

        // Ordered by tree position so the computation is deterministic, not map order.
        let mut selected: Vec<PositionedInput> = latest_per_slot.into_values().collect();
        selected.sort_by_key(|(index, _)| *index);
        Ok(selected.into_iter().map(|(_, item)| item).collect())
    }

    /// `sha256(sha256(bytes) || commitment || slot) mod SNARK_SCALAR_FIELD`, hex-encoded.
    ///
    /// Must stay byte-identical to `CRISPProgram.inputLeaf`, or no root will ever match.
    fn input_leaf(ciphertext: &[u8], commitment: &[u8; 32], slot: &[u8; 20]) -> String {
        let bytes_digest = Sha256::digest(ciphertext);

        let mut outer = Sha256::new();
        outer.update(bytes_digest);
        outer.update(commitment);
        outer.update(slot);
        let combined = outer.finalize();

        let field = BigUint::from_str_radix(SNARK_SCALAR_FIELD, 10).expect("field constant");
        let reduced = BigUint::from_bytes_be(&combined) % field;
        hex::encode(reduced.to_bytes_be())
    }

    fn poseidon_hash(nodes: Vec<String>) -> String {
        let mut poseidon = Poseidon::<Fr>::new_circom(2).unwrap();
        let mut field_elements = Vec::new();

        for node in nodes {
            let sanitized_node = node.trim_start_matches("0x");
            let numeric_str = BigUint::from_str_radix(sanitized_node, 16)
                .unwrap()
                .to_string();
            let field_repr = Fr::from_str(&numeric_str).unwrap();
            field_elements.push(field_repr);
        }

        let result_hash: BigInt<4> = poseidon.hash(&field_elements).unwrap().into();
        hex::encode(result_hash.to_bytes_be())
    }

    pub fn build_tree(&self) -> Result<IMT, ComputeError> {
        let mut tree = IMT::new(
            Self::poseidon_hash,
            self.depth,
            self.zero_value.clone(),
            self.arity,
            vec![],
        )
        .map_err(|e| ComputeError::MerkleTree(e.to_string()))?;

        for leaf in &self.leaf_hashes {
            tree.insert(leaf.clone())
                .map_err(|e| ComputeError::MerkleTree(e.to_string()))?;
        }

        Ok(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::MerkleTreeBuilder;

    #[test]
    fn test_depth_computation() {
        assert_eq!(MerkleTreeBuilder::new(0).depth, 1);
        assert_eq!(MerkleTreeBuilder::new(1).depth, 1);
        assert_eq!(MerkleTreeBuilder::new(2).depth, 1);
        assert_eq!(MerkleTreeBuilder::new(3).depth, 2);
        assert_eq!(MerkleTreeBuilder::new(4).depth, 2);
        assert_eq!(MerkleTreeBuilder::new(5).depth, 3);
        assert_eq!(MerkleTreeBuilder::new(8).depth, 3);
        assert_eq!(MerkleTreeBuilder::new(9).depth, 4);
        assert_eq!(MerkleTreeBuilder::new(16).depth, 4);
        assert_eq!(MerkleTreeBuilder::new(17).depth, 5);
    }

    #[test]
    fn one_zero_leaf_matches_solidity_lazy_imt() {
        let root = MerkleTreeBuilder::new(1)
            .with_leaf_hashes(vec!["0".to_string()])
            .build_tree()
            .unwrap()
            .root()
            .unwrap();

        assert_eq!(
            root,
            "2098f5fb9e239eab3ceac3f27b81e481dc3124d55ffed523a839ee8446b64864"
        );
    }
}
