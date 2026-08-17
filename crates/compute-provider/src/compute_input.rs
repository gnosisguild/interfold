// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::ciphertext_output::ComputeResult;
use crate::merkle_tree_builder::MerkleTreeBuilder;
use e3_bfv_client::client::compute_ct_commitment;
use e3_fhe_params::decode_bfv_params;
use sha3::{Digest, Keccak256};

pub type FHEProcessor = fn(&FHEInputs) -> Vec<u8>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FHEInputs {
    pub ciphertexts: Vec<(Vec<u8>, u64)>,
    pub params: Vec<u8>,
}

/// The full input to the Secure Process.
///
/// This type holds only the values the Secure Process computes over. Every field the journal
/// publishes is derived from these values inside the compute environment. A prover cannot supply
/// the input Merkle root or the output hash as separate values, because a separate value can
/// disagree with the ciphertexts the Secure Process actually consumed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ComputeInput {
    pub fhe_inputs: FHEInputs,
}

impl ComputeInput {
    pub fn process(&self, fhe_processor: FHEProcessor) -> ComputeResult {
        let processed_ciphertext = (fhe_processor)(&self.fhe_inputs);
        let processed_hash = Keccak256::digest(&processed_ciphertext).to_vec();
        let params =
            decode_bfv_params(&self.fhe_inputs.params).expect("Failed to decode BFV params");
        let ciphertext_commitment = compute_ct_commitment(
            processed_ciphertext.clone(),
            params.degree(),
            params.plaintext(),
            params.moduli().to_vec(),
        )
        .expect("Failed to compute ciphertext commitment")
        .to_vec();
        let params_hash = Keccak256::digest(&self.fhe_inputs.params).to_vec();

        // Derive the leaves from the ciphertexts this Secure Process consumed. The E3 program
        // compares the resulting root against the input root it accumulated on chain, so the
        // comparison rejects a result computed over any other input set.
        let mut tree_builder = MerkleTreeBuilder::new(self.fhe_inputs.ciphertexts.len());
        tree_builder.compute_leaf_hashes(&self.fhe_inputs.ciphertexts, &self.fhe_inputs.params);
        let merkle_root = tree_builder.build_tree().root().unwrap();

        ComputeResult {
            ciphertext_hash: processed_hash,
            ciphertext_commitment,
            params_hash,
            merkle_root: hex::decode(merkle_root).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use e3_fhe_params::{build_pair_for_preset, encode_bfv_params, BfvPreset};
    use fhe::bfv::{Ciphertext, Encoding, Plaintext, PublicKey, SecretKey};
    use fhe_traits::{FheEncoder, FheEncrypter, Serialize as FheSerialize};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::sync::Arc;

    fn sum_processor(inputs: &FHEInputs) -> Vec<u8> {
        let params = Arc::new(decode_bfv_params(&inputs.params).unwrap());
        let mut sum = Ciphertext::zero(&params);
        for (bytes, _) in &inputs.ciphertexts {
            use fhe_traits::DeserializeParametrized;
            sum += &Ciphertext::from_bytes(bytes, &params).unwrap();
        }
        sum.to_bytes()
    }

    /// Builds `values.len()` encrypted inputs under one key, plus the encoded parameters.
    fn encrypted_inputs(values: &[u64]) -> FHEInputs {
        let (params, _) = build_pair_for_preset(BfvPreset::InsecureThreshold512).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let secret_key = SecretKey::random(&params, &mut rng);
        let public_key = PublicKey::new(&secret_key, &mut rng);

        let ciphertexts = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let plaintext =
                    Plaintext::try_encode(&[*value], Encoding::poly(), &params).unwrap();
                let ciphertext = public_key.try_encrypt(&plaintext, &mut rng).unwrap();
                (ciphertext.to_bytes(), index as u64)
            })
            .collect();

        FHEInputs {
            ciphertexts,
            params: encode_bfv_params(&params),
        }
    }

    fn root_over(inputs: &FHEInputs) -> Vec<u8> {
        let mut builder = MerkleTreeBuilder::new(inputs.ciphertexts.len());
        builder.compute_leaf_hashes(&inputs.ciphertexts, &inputs.params);
        hex::decode(builder.build_tree().root().unwrap()).unwrap()
    }

    /// The journal's input root must be a function of the ciphertexts the Secure Process consumed.
    /// Before this binding existed, the root arrived as a separate `leaf_hashes` field, so a
    /// prover could publish a tally over one input set while proving the root of another.
    #[test]
    fn merkle_root_is_derived_from_the_processed_ciphertexts() {
        let inputs = encrypted_inputs(&[1, 1, 1]);
        let result = ComputeInput {
            fhe_inputs: inputs.clone(),
        }
        .process(sum_processor);

        assert_eq!(result.merkle_root, root_over(&inputs));
    }

    /// Changing the consumed ciphertexts must change the published root, so an E3 program that
    /// compares the root against its own on-chain root rejects the substituted set.
    #[test]
    fn substituted_ciphertexts_produce_a_different_root() {
        let honest = encrypted_inputs(&[1, 1, 1]);
        let mut forged = honest.clone();
        // Same leaf count, so the tree depth is unchanged; only the ciphertexts differ.
        forged.ciphertexts[2] = encrypted_inputs(&[9]).ciphertexts[0].clone();

        let honest_root = ComputeInput { fhe_inputs: honest }
            .process(sum_processor)
            .merkle_root;
        let forged_root = ComputeInput { fhe_inputs: forged }
            .process(sum_processor)
            .merkle_root;

        assert_ne!(honest_root, forged_root);
    }
}
