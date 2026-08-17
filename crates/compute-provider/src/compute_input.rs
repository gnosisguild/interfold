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
    /// The serialized ciphertexts to process, each paired with its on-chain index.
    ///
    /// The index is not the authority on input order — a leaf's position in the tree is its
    /// position in this vector, because `MerkleTreeBuilder::compute_leaf_hashes` walks the vector
    /// in order and the tree inserts sequentially. **A caller assembling this vector from chain
    /// events must sort by the index**, because event delivery is not ordered; getting it wrong
    /// produces a root the E3 program rejects.
    pub ciphertexts: Vec<(Vec<u8>, u64)>,
    /// The commitment the E3 program stored for each ciphertext, in the same order.
    ///
    /// The proof an E3 program checks at input time constrains the commitment, never the
    /// serialized bytes above. Carrying both lets the Secure Process recompute each commitment
    /// from the bytes it consumed and drop any input where the two disagree.
    ///
    /// Empty means "no commitments supplied": every input is then processed without the check,
    /// which is the pre-binding behaviour and only safe for an E3 program whose inputs cannot
    /// carry unbound bytes.
    #[serde(default)]
    pub commitments: Vec<[u8; 32]>,
    /// The slot each input was published to, in the same order.
    ///
    /// The input tree is append-only, so a slot can hold several entries and the Secure Process
    /// tallies the most recent one whose bytes match its commitment. That grouping has to be bound
    /// by the root, which is why the slot is part of the leaf.
    ///
    /// Supplied together with `commitments`; when either is empty no check or grouping is done and
    /// every input is processed, which is the pre-binding behaviour.
    #[serde(default)]
    pub slots: Vec<[u8; 20]>,
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

/// A failure inside the Secure Process.
///
/// These arise from malformed inputs, which a Secure Process cannot assume away: an E3 program
/// accepts inputs from untrusted parties, and a single unusable one reaches the compute environment
/// like any other. Returning the reason lets a compute provider report which input was bad rather
/// than aborting the process with a panic.
#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    #[error("failed to decode BFV parameters: {0}")]
    DecodeParams(String),

    #[error("failed to compute the commitment of ciphertext {index}: {reason}")]
    LeafCommitment { index: usize, reason: String },

    #[error("failed to compute the commitment of the output ciphertext: {0}")]
    OutputCommitment(String),

    #[error("failed to build the input Merkle tree: {0}")]
    MerkleTree(String),
}

impl ComputeInput {
    pub fn process(&self, fhe_processor: FHEProcessor) -> Result<ComputeResult, ComputeError> {
        let params = decode_bfv_params(&self.fhe_inputs.params)
            .map_err(|e| ComputeError::DecodeParams(e.to_string()))?;

        // Every leaf is derived here, from the exact bytes, commitments and slots this Secure
        // Process was given, so the root the E3 program compares against binds all three. An input
        // whose bytes do not reproduce its commitment still contributes its leaf — dropping it
        // would change the root — but it is not computed over.
        let mut tree_builder = MerkleTreeBuilder::new(self.fhe_inputs.ciphertexts.len());
        let usable = tree_builder.compute_leaf_hashes(&self.fhe_inputs, &params)?;
        let merkle_root = tree_builder
            .build_tree()
            .map_err(|e| ComputeError::MerkleTree(e.to_string()))?
            .root()
            .ok_or_else(|| ComputeError::MerkleTree("the tree has no root".into()))?;

        // The processor sees only the inputs that matched their commitment. Both the root above
        // and this filter are functions of values the root binds, so the excluded set is
        // reproducible by anyone holding the same inputs — a prover cannot choose it.
        let processed_ciphertext = (fhe_processor)(&FHEInputs {
            ciphertexts: usable,
            commitments: Vec::new(),
            slots: Vec::new(),
            params: self.fhe_inputs.params.clone(),
        });
        let processed_hash = Keccak256::digest(&processed_ciphertext).to_vec();
        let ciphertext_commitment = compute_ct_commitment(
            processed_ciphertext.clone(),
            params.degree(),
            params.plaintext(),
            params.moduli().to_vec(),
        )
        .map_err(|e| ComputeError::OutputCommitment(e.to_string()))?
        .to_vec();
        let params_hash = Keccak256::digest(&self.fhe_inputs.params).to_vec();

        Ok(ComputeResult {
            ciphertext_hash: processed_hash,
            ciphertext_commitment,
            params_hash,
            merkle_root: hex::decode(merkle_root)
                .map_err(|e| ComputeError::MerkleTree(e.to_string()))?,
        })
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
            commitments: Vec::new(),
            slots: Vec::new(),
            params: encode_bfv_params(&params),
        }
    }

    /// Attaches the commitment and slot an E3 program would have stored for each ciphertext.
    /// Each input gets its own slot unless `slots` is given.
    fn with_real_commitments(inputs: FHEInputs) -> FHEInputs {
        let count = inputs.ciphertexts.len();
        with_slots(inputs, (0..count).map(|i| slot(i as u8)).collect())
    }

    /// A distinct slot address per index.
    fn slot(tag: u8) -> [u8; 20] {
        let mut address = [0u8; 20];
        address[19] = tag;
        address
    }

    fn with_slots(mut inputs: FHEInputs, slots: Vec<[u8; 20]>) -> FHEInputs {
        let params = decode_bfv_params(&inputs.params).unwrap();
        inputs.commitments = inputs
            .ciphertexts
            .iter()
            .map(|(bytes, _)| {
                compute_ct_commitment(
                    bytes.clone(),
                    params.degree(),
                    params.plaintext(),
                    params.moduli().to_vec(),
                )
                .unwrap()
            })
            .collect();
        inputs.slots = slots;
        inputs
    }

    fn root_over(inputs: &FHEInputs) -> Vec<u8> {
        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(inputs.ciphertexts.len());
        builder.compute_leaf_hashes(inputs, &params).unwrap();
        hex::decode(builder.build_tree().unwrap().root().unwrap()).unwrap()
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
        .process(sum_processor)
        .unwrap();

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
            .unwrap()
            .merkle_root;
        let forged_root = ComputeInput { fhe_inputs: forged }
            .process(sum_processor)
            .unwrap()
            .merkle_root;

        assert_ne!(honest_root, forged_root);
    }

    /// A Secure Process accepts inputs from untrusted parties, so an unusable one must surface as
    /// an error naming the offending index, not as a panic that aborts the prover.
    #[test]
    fn a_malformed_input_reports_its_index_instead_of_panicking() {
        let mut inputs = encrypted_inputs(&[1, 1, 1]);
        inputs.ciphertexts[1].0 = vec![0xff; 8];

        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(inputs.ciphertexts.len());
        let error = builder.compute_leaf_hashes(&inputs, &params).unwrap_err();

        assert!(
            matches!(error, ComputeError::LeafCommitment { index: 1, .. }),
            "expected LeafCommitment at index 1, got {error:?}"
        );
    }

    /// An input whose published bytes do not reproduce its stored commitment must be excluded from
    /// the computation while still contributing its leaf. Before this binding, the honest prover
    /// simply could not reproduce the on-chain root and the whole round was lost.
    #[test]
    fn an_input_whose_bytes_contradict_its_commitment_is_excluded_from_the_tally() {
        let honest = with_real_commitments(encrypted_inputs(&[1, 1, 1]));

        // As published on chain: the commitment of input 1 is real and proven, but the bytes beside
        // it are a different ciphertext. The contract builds its leaf from exactly this pair, so
        // the guest sees the same pair and derives the same leaf.
        let mut attacked = honest.clone();
        attacked.ciphertexts[1].0 = encrypted_inputs(&[9]).ciphertexts[0].0.clone();

        let result = ComputeInput {
            fhe_inputs: attacked.clone(),
        }
        .process(sum_processor)
        .expect("a contradicting input must not stop the round");

        // The tally must be exactly the two inputs that did match, so it equals a run over only
        // those two.
        let mut only_matching = honest.clone();
        only_matching.ciphertexts.remove(1);
        only_matching.commitments.remove(1);
        only_matching.slots.remove(1);
        let reference = ComputeInput {
            fhe_inputs: only_matching,
        }
        .process(sum_processor)
        .unwrap();

        assert_eq!(
            result.ciphertext_hash, reference.ciphertext_hash,
            "the contradicting input must not reach the processor"
        );

        // And it is still represented in the tree, so the root differs from the two-input run.
        assert_ne!(result.merkle_root, reference.merkle_root);
    }

    /// Undecodable bytes are an unusable input, not a failure: the round must survive them.
    #[test]
    fn undecodable_bytes_are_excluded_rather_than_fatal() {
        let mut inputs = with_real_commitments(encrypted_inputs(&[1, 1, 1]));
        inputs.ciphertexts[2].0 = vec![0xff; 8];

        let result = ComputeInput { fhe_inputs: inputs }
            .process(sum_processor)
            .expect("garbage bytes must not abort the Secure Process");

        assert_eq!(result.merkle_root.len(), 32);
    }

    /// The leaf layout must match `CRISPProgram.inputLeaf`, byte for byte.
    ///
    /// This vector is duplicated in
    /// `examples/CRISP/packages/crisp-contracts/tests/input-leaf.test.ts`. A divergence between
    /// the two implementations makes every root mismatch, and nothing else would catch it.
    #[test]
    fn leaf_layout_matches_the_contract() {
        let ciphertext: Vec<u8> = (0u8..64).collect();
        let commitment = [0xabu8; 32];

        let inputs = FHEInputs {
            ciphertexts: vec![(ciphertext, 0)],
            commitments: vec![commitment],
            slots: vec![[0xcdu8; 20]],
            params: encode_bfv_params(
                &build_pair_for_preset(BfvPreset::InsecureThreshold512)
                    .unwrap()
                    .0,
            ),
        };

        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(1);
        builder.compute_leaf_hashes(&inputs, &params).unwrap();

        let expected = num_bigint::BigUint::parse_bytes(
            b"3005744733328395831398716072572247490877798047068525662443106668216528579058",
            10,
        )
        .unwrap();
        assert_eq!(
            num_bigint::BigUint::from_bytes_be(&hex::decode(&builder.leaf_hashes[0]).unwrap()),
            expected,
            "leaf layout diverged from CRISPProgram.inputLeaf"
        );
    }

    /// A commitment list of the wrong length is a caller bug that would silently mis-pair inputs
    /// with commitments, so it must be rejected rather than guessed at.
    #[test]
    fn a_mismatched_commitment_count_is_rejected() {
        let mut inputs = with_real_commitments(encrypted_inputs(&[1, 1, 1]));
        inputs.commitments.pop();

        let params = decode_bfv_params(&inputs.params).unwrap();
        let error = MerkleTreeBuilder::new(3)
            .compute_leaf_hashes(&inputs, &params)
            .unwrap_err();

        assert!(
            matches!(error, ComputeError::MerkleTree(_)),
            "got {error:?}"
        );
    }

    /// A round where *every* input is unusable degenerates into a round with nothing to tally.
    ///
    /// The processor returns an empty ciphertext, which does not serialize back into a valid one,
    /// so the output commitment fails with a typed error rather than a panic. That is acceptable:
    /// this state is only reachable when no honest input exists — one honest input is never
    /// excluded, because its bytes reproduce its commitment — and it is indistinguishable from a
    /// round that received no inputs at all, which the protocol already resolves as
    /// `NoInputsReceived`. Asserted so the behaviour is a decision rather than an accident.
    #[test]
    fn a_round_where_every_input_is_unusable_fails_cleanly() {
        let mut inputs = with_real_commitments(encrypted_inputs(&[1, 1]));
        for entry in inputs.ciphertexts.iter_mut() {
            entry.0 = vec![0xde, 0xad, 0xbe, 0xef];
        }

        let error = ComputeInput { fhe_inputs: inputs }
            .process(sum_processor)
            .unwrap_err();

        assert!(
            matches!(error, ComputeError::OutputCommitment(_)),
            "expected a typed output-commitment error, got {error:?}"
        );
    }

    /// A single honest input must survive any number of unusable ones beside it. This is the
    /// property that turns a round-killing input into a excluded one.
    #[test]
    fn one_honest_input_still_produces_a_tally_among_unusable_ones() {
        let mut inputs = with_real_commitments(encrypted_inputs(&[1, 1, 1]));
        inputs.ciphertexts[0].0 = vec![0x00; 12];
        inputs.ciphertexts[2].0 = vec![0xff; 5];

        let result = ComputeInput {
            fhe_inputs: inputs.clone(),
        }
        .process(sum_processor)
        .expect("one usable input must be enough");

        // The tally is exactly the surviving input.
        let mut only_good = inputs.clone();
        only_good.ciphertexts = vec![inputs.ciphertexts[1].clone()];
        only_good.commitments = vec![inputs.commitments[1]];
        only_good.slots = vec![inputs.slots[1]];
        let reference = ComputeInput {
            fhe_inputs: only_good,
        }
        .process(sum_processor)
        .unwrap();

        assert_eq!(result.ciphertext_hash, reference.ciphertext_hash);
    }

    /// The excluded set must be a function of the inputs alone, so two provers over the same
    /// published data produce the same result and neither can choose what to drop.
    #[test]
    fn the_excluded_set_is_deterministic() {
        let mut inputs = with_real_commitments(encrypted_inputs(&[1, 1, 1]));
        inputs.ciphertexts[0].0 = vec![0x00; 16];

        let first = ComputeInput {
            fhe_inputs: inputs.clone(),
        }
        .process(sum_processor)
        .unwrap();
        let second = ComputeInput { fhe_inputs: inputs }
            .process(sum_processor)
            .unwrap();

        assert_eq!(first.merkle_root, second.merkle_root);
        assert_eq!(first.ciphertext_hash, second.ciphertext_hash);
        assert_eq!(first.ciphertext_commitment, second.ciphertext_commitment);
    }

    /// Swapping the commitment beside honest bytes must change the leaf, or an attacker could
    /// pair any commitment with any ciphertext.
    #[test]
    fn the_leaf_binds_the_commitment_as_well_as_the_bytes() {
        let inputs = with_real_commitments(encrypted_inputs(&[1]));
        let params = decode_bfv_params(&inputs.params).unwrap();

        let mut honest = MerkleTreeBuilder::new(1);
        honest.compute_leaf_hashes(&inputs, &params).unwrap();

        let mut swapped_inputs = inputs.clone();
        swapped_inputs.commitments[0] = [0x11u8; 32];
        let mut swapped = MerkleTreeBuilder::new(1);
        swapped
            .compute_leaf_hashes(&swapped_inputs, &params)
            .unwrap();

        assert_ne!(honest.leaf_hashes[0], swapped.leaf_hashes[0]);
    }

    /// With no commitments or slots the builder keeps the pre-binding layout, so an E3 program
    /// that has not migrated still works.
    #[test]
    fn without_binding_the_leaf_is_the_ciphertext_commitment() {
        let inputs = encrypted_inputs(&[5]);
        let bound = with_real_commitments(inputs.clone());
        let params = decode_bfv_params(&inputs.params).unwrap();

        let mut unchecked = MerkleTreeBuilder::new(1);
        let usable = unchecked.compute_leaf_hashes(&inputs, &params).unwrap();

        assert_eq!(usable.len(), 1);
        assert_eq!(
            unchecked.leaf_hashes[0],
            hex::encode(bound.commitments[0]),
            "the unbound leaf is the bare ciphertext commitment"
        );
    }

    /// The slot is part of the leaf, so the same ciphertext published to a different slot is a
    /// different leaf. Without this a prover could re-group entries and change which one wins.
    #[test]
    fn the_leaf_binds_the_slot() {
        let base = encrypted_inputs(&[3]);
        let params = decode_bfv_params(&base.params).unwrap();

        let a = with_slots(base.clone(), vec![slot(1)]);
        let b = with_slots(base, vec![slot(2)]);

        let mut first = MerkleTreeBuilder::new(1);
        first.compute_leaf_hashes(&a, &params).unwrap();
        let mut second = MerkleTreeBuilder::new(1);
        second.compute_leaf_hashes(&b, &params).unwrap();

        assert_ne!(first.leaf_hashes[0], second.leaf_hashes[0]);
    }

    /// The heart of append-only: a later entry that contradicts its commitment must not erase the
    /// good entry already in the slot. This is the mask-poisoning case.
    #[test]
    fn a_poisoned_later_entry_falls_back_to_the_slot_s_last_good_one() {
        let victim = slot(7);
        let honest = encrypted_inputs(&[4]);
        let mut inputs = with_slots(honest.clone(), vec![victim]);

        // An attacker appends a second entry to the victim's slot: real commitment, wrong bytes.
        let poison = encrypted_inputs(&[9]);
        inputs
            .ciphertexts
            .push((poison.ciphertexts[0].0.clone(), 1));
        inputs.commitments.push(inputs.commitments[0]);
        inputs.slots.push(victim);

        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(inputs.ciphertexts.len());
        let usable = builder.compute_leaf_hashes(&inputs, &params).unwrap();

        assert_eq!(
            usable.len(),
            1,
            "the slot still contributes exactly one entry"
        );
        assert_eq!(
            usable[0].0, honest.ciphertexts[0].0,
            "the victim's original vote must survive"
        );
        assert_eq!(
            builder.leaf_hashes.len(),
            2,
            "both entries stay in the tree"
        );
    }

    /// An honest re-vote must replace the earlier one, or voters could never change their mind.
    #[test]
    fn an_honest_later_entry_replaces_the_earlier_one() {
        let voter = slot(3);
        let first = encrypted_inputs(&[1]);
        let second = encrypted_inputs(&[8]);

        let mut inputs = with_slots(first, vec![voter]);
        let later = with_slots(second.clone(), vec![voter]);
        inputs.ciphertexts.push((later.ciphertexts[0].0.clone(), 1));
        inputs.commitments.push(later.commitments[0]);
        inputs.slots.push(voter);

        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(2);
        let usable = builder.compute_leaf_hashes(&inputs, &params).unwrap();

        assert_eq!(usable.len(), 1);
        assert_eq!(usable[0].0, second.ciphertexts[0].0, "the re-vote wins");
    }

    /// Distinct slots each contribute their own entry, in tree order.
    #[test]
    fn every_slot_contributes_its_latest_usable_entry() {
        let inputs = with_real_commitments(encrypted_inputs(&[1, 2, 3]));
        let params = decode_bfv_params(&inputs.params).unwrap();

        let mut builder = MerkleTreeBuilder::new(3);
        let usable = builder.compute_leaf_hashes(&inputs, &params).unwrap();

        assert_eq!(usable.len(), 3);
        assert_eq!(usable[0].1, 0);
        assert_eq!(usable[2].1, 2);
    }

    /// Builds one slot's entry sequence. `true` is an honest entry, `false` an entry whose bytes
    /// do not reproduce its commitment. Returns the inputs and the honest ciphertexts by index.
    fn sequence_for_one_slot(pattern: &[bool]) -> (FHEInputs, Vec<Vec<u8>>) {
        let target = slot(9);
        let ballots: Vec<Vec<u8>> = (0..pattern.len())
            .map(|i| encrypted_inputs(&[(i as u64) + 1]).ciphertexts[0].0.clone())
            .collect();

        // Distinct ballots, or a test expecting "the later honest entry" would pass even if the
        // earlier one were selected.
        for (i, ballot) in ballots.iter().enumerate() {
            for other in &ballots[i + 1..] {
                assert_ne!(ballot, other, "sequence ballots must be distinguishable");
            }
        }

        let mut inputs = encrypted_inputs(&[1]);
        inputs.ciphertexts.clear();
        let params = decode_bfv_params(&inputs.params).unwrap();
        let commit = |bytes: &Vec<u8>| {
            compute_ct_commitment(
                bytes.clone(),
                params.degree(),
                params.plaintext(),
                params.moduli().to_vec(),
            )
            .unwrap()
        };

        for (index, honest) in pattern.iter().enumerate() {
            // A poisoned entry keeps a real commitment and carries unrelated bytes, which is what a
            // third party can publish and no on-chain check can reject.
            let published = if *honest {
                ballots[index].clone()
            } else {
                vec![0xde, 0xad, 0xbe, 0xef, index as u8]
            };
            inputs.ciphertexts.push((published, index as u64));
            inputs.commitments.push(commit(&ballots[index]));
            inputs.slots.push(target);
        }
        (inputs, ballots)
    }

    fn selected_for(pattern: &[bool]) -> (Vec<(Vec<u8>, u64)>, Vec<Vec<u8>>, usize) {
        let (inputs, ballots) = sequence_for_one_slot(pattern);
        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(inputs.ciphertexts.len());
        let selected = builder.compute_leaf_hashes(&inputs, &params).unwrap();
        (selected, ballots, builder.leaf_hashes.len())
    }

    /// Every entry sequence for one slot must resolve to that slot's most recent honest entry, and
    /// every entry must stay in the tree regardless.
    ///
    /// The case that matters most is honest → poisoned → honest: selection has to move *forward*
    /// to the later honest entry, not fall back to the first one.
    #[test]
    fn selection_picks_the_latest_honest_entry_in_every_sequence() {
        // (pattern, expected selected index, description)
        let cases: &[(&[bool], Option<usize>, &str)] = &[
            (&[true], Some(0), "a single honest entry"),
            (&[false], None, "a single poisoned entry contributes nothing"),
            (&[true, false], Some(0), "poisoned append falls back"),
            (&[false, true], Some(1), "an honest entry after a poisoned one wins"),
            (&[true, true], Some(1), "an honest re-vote replaces"),
            (&[true, false, true], Some(2), "honest, poisoned, honest picks the last honest"),
            (&[true, true, false], Some(1), "poisoned at the end falls back one step"),
            (&[true, false, false], Some(0), "two poisoned appends still fall back to the first"),
            (&[false, false, true], Some(2), "honest after two poisoned wins"),
            (&[false, true, false], Some(1), "honest in the middle survives a later poison"),
            (&[true, false, true, false], Some(2), "falls back past a trailing poison"),
            (&[false, false, false], None, "an all-poisoned slot contributes nothing"),
        ];

        for (pattern, expected, description) in cases {
            let (selected, ballots, leaves) = selected_for(pattern);

            assert_eq!(
                leaves,
                pattern.len(),
                "every entry must stay in the tree ({description})"
            );

            match expected {
                Some(index) => {
                    assert_eq!(selected.len(), 1, "one entry per slot ({description})");
                    assert_eq!(
                        selected[0].0, ballots[*index],
                        "expected entry {index} ({description})"
                    );
                }
                None => assert!(
                    selected.is_empty(),
                    "no entry should be selected ({description})"
                ),
            }
        }
    }

    /// Sequences interleaved across slots must not bleed into one another: each slot resolves on
    /// its own entries, in tree order.
    #[test]
    fn interleaved_slots_resolve_independently() {
        let (mut a, ballots_a) = sequence_for_one_slot(&[true, false]);
        let (b, ballots_b) = sequence_for_one_slot(&[false, true]);
        let other = slot(11);

        // Interleave: A-honest, B-poisoned, A-poisoned, B-honest.
        let order = [(0usize, false), (0, true), (1, false), (1, true)];
        let mut inputs = a.clone();
        inputs.ciphertexts.clear();
        inputs.commitments.clear();
        inputs.slots.clear();
        for (position, (index, from_b)) in order.iter().enumerate() {
            let source = if *from_b { &b } else { &a };
            inputs
                .ciphertexts
                .push((source.ciphertexts[*index].0.clone(), position as u64));
            inputs.commitments.push(source.commitments[*index]);
            inputs.slots.push(if *from_b { other } else { slot(9) });
        }
        a.ciphertexts.clear();

        let params = decode_bfv_params(&inputs.params).unwrap();
        let mut builder = MerkleTreeBuilder::new(4);
        let selected = builder.compute_leaf_hashes(&inputs, &params).unwrap();

        assert_eq!(builder.leaf_hashes.len(), 4, "all four entries stay in the tree");
        assert_eq!(selected.len(), 2, "one entry per slot");
        // Ordered by tree position: slot 9's honest entry is at 0, slot 11's at 3.
        assert_eq!(selected[0].0, ballots_a[0]);
        assert_eq!(selected[1].0, ballots_b[1]);
    }
}
