// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Runs the CRISP Secure Process natively, outside the RISC Zero zkVM.
//!
//! The guest is one line — `input.input.process(fhe_processor)` — so calling that here exercises
//! the same code the zkVM runs, with the real CRISP processor rather than a stand-in. Everything
//! except proof generation is covered, which matters because a guest failure inside the zkVM
//! surfaces only as a missing proof and a requester-billed compute timeout.
//!
//! These tests are the reason the byte-to-commitment binding exists: an input whose published bytes
//! are not the ciphertext that was proven used to abort the whole round here.

use e3_compute_provider::{ComputeError, ComputeInput, FHEInputs};
use e3_fhe_params::{build_pair_for_preset, encode_bfv_params, BfvPreset};
use e3_user_program::fhe_processor;
use fhe::bfv::{BfvParameters, Ciphertext, Encoding, Plaintext, PublicKey, SecretKey};
use fhe_traits::{
    DeserializeParametrized, FheDecoder, FheDecrypter, FheEncoder, FheEncrypter,
    Serialize as FheSerialize,
};
use rand::{rngs::StdRng, SeedableRng};
use std::sync::Arc;

const PRESET: BfvPreset = BfvPreset::InsecureThreshold512;

struct Round {
    params: Arc<BfvParameters>,
    secret_key: SecretKey,
    public_key: PublicKey,
}

impl Round {
    fn new() -> Self {
        let (params, _) = build_pair_for_preset(PRESET).unwrap();
        let mut rng = StdRng::seed_from_u64(42);
        let secret_key = SecretKey::random(&params, &mut rng);
        let public_key = PublicKey::new(&secret_key, &mut rng);
        Self {
            params,
            secret_key,
            public_key,
        }
    }

    /// Encrypts one ballot the way a voter's client would.
    fn ballot(&self, votes: &[u64], nonce: u64) -> Vec<u8> {
        let mut rng = StdRng::seed_from_u64(1_000 + nonce);
        let plaintext = Plaintext::try_encode(votes, Encoding::poly(), &self.params).unwrap();
        let ciphertext: Ciphertext = self.public_key.try_encrypt(&plaintext, &mut rng).unwrap();
        ciphertext.to_bytes()
    }

    /// The commitment an E3 program stores for a ciphertext.
    fn commitment(&self, bytes: &[u8]) -> [u8; 32] {
        e3_bfv_client::client::compute_ct_commitment(
            bytes.to_vec(),
            self.params.degree(),
            self.params.plaintext(),
            self.params.moduli().to_vec(),
        )
        .unwrap()
    }

    /// One ballot per slot, which is the ordinary case.
    fn inputs(&self, ballots: Vec<Vec<u8>>) -> FHEInputs {
        let slots = (0..ballots.len()).map(|i| Self::slot(i as u8)).collect();
        self.inputs_at(ballots, slots)
    }

    fn slot(tag: u8) -> [u8; 20] {
        let mut address = [0u8; 20];
        address[19] = tag;
        address
    }

    fn inputs_at(&self, ballots: Vec<Vec<u8>>, slots: Vec<[u8; 20]>) -> FHEInputs {
        let commitments = ballots.iter().map(|b| self.commitment(b)).collect();
        FHEInputs {
            slots,
            ciphertexts: ballots
                .into_iter()
                .enumerate()
                .map(|(i, b)| (b, i as u64))
                .collect(),
            commitments,
            params: encode_bfv_params(&self.params),
        }
    }

    /// Decrypts a tally ciphertext, as the ciphernode committee would.
    ///
    /// `ComputeResult` carries only the keccak digest of the output, so the output ciphertext is
    /// taken from the processor directly. Comparing digests is exact where only equality matters.
    fn decrypt_tally(&self, ciphertext_bytes: &[u8], options: usize) -> Vec<u64> {
        // Against `self.params`, not a re-decoded copy: fhe.rs compares parameters by `Arc`
        // identity, so a structurally identical clone is rejected as incompatible.
        let ciphertext = Ciphertext::from_bytes(ciphertext_bytes, &self.params).unwrap();
        let plaintext = self.secret_key.try_decrypt(&ciphertext).unwrap();
        Vec::<u64>::try_decode(&plaintext, Encoding::poly()).unwrap()[..options].to_vec()
    }
}

/// The honest path: every input is well formed and the tally is their homomorphic sum.
#[test]
fn the_secure_process_tallies_well_formed_ballots() {
    let round = Round::new();
    let inputs = round.inputs(vec![
        round.ballot(&[3, 0], 1),
        round.ballot(&[0, 5], 2),
        round.ballot(&[2, 0], 3),
    ]);

    let result = ComputeInput {
        fhe_inputs: inputs.clone(),
    }
    .process(fhe_processor)
    .expect("an honest round must process");

    // Every input is usable, so the processor output over the whole set is the tally.
    let tally = round.decrypt_tally(&fhe_processor(&inputs), 2);
    assert_eq!(tally, vec![5, 5], "3+2 for option 0 and 5 for option 1");
    assert_eq!(result.merkle_root.len(), 32);
}

/// An input whose published bytes are not the ciphertext that was proven is dropped from the
/// tally, and the round still completes.
///
/// This is the case that used to abort the guest: `fhe_processor` deserializes with `unwrap`, so
/// the bad blob panicked before any error path could run. The binding now keeps it away from the
/// processor entirely.
#[test]
fn a_contradicting_input_is_dropped_and_the_round_survives() {
    let round = Round::new();
    let honest = vec![
        round.ballot(&[3, 0], 1),
        round.ballot(&[0, 5], 2),
        round.ballot(&[2, 0], 3),
    ];

    let mut attacked = round.inputs(honest.clone());
    // A real, proven commitment beside bytes that are not its ciphertext — exactly what an E3
    // program cannot detect on chain.
    attacked.ciphertexts[1].0 = round.ballot(&[0, 99], 9);

    let result = ComputeInput {
        fhe_inputs: attacked,
    }
    .process(fhe_processor)
    .expect("a contradicting input must not abort the Secure Process");

    // The tally is the two inputs that matched, and nothing from the substituted one.
    let mut only_matching = round.inputs_at(
        vec![honest[0].clone(), honest[2].clone()],
        vec![Round::slot(0), Round::slot(2)],
    );
    only_matching.ciphertexts[1].1 = 2;
    let reference = ComputeInput {
        fhe_inputs: only_matching,
    }
    .process(fhe_processor)
    .unwrap();

    assert_eq!(
        result.ciphertext_hash, reference.ciphertext_hash,
        "the substituted ballot must not reach the tally"
    );

    // And the surviving ballots are the honest ones: 3 and 2 for option 0, nothing for option 1.
    let mut survivors = round.inputs_at(
        vec![honest[0].clone(), honest[2].clone()],
        vec![Round::slot(0), Round::slot(2)],
    );
    survivors.ciphertexts[1].1 = 2;
    assert_eq!(
        round.decrypt_tally(&fhe_processor(&survivors), 2),
        vec![5, 0]
    );
}

/// Undecodable bytes reach the same outcome. Without the binding this is the cheapest way to kill
/// a round: one input of garbage and the guest aborts.
#[test]
fn garbage_bytes_do_not_abort_the_secure_process() {
    let round = Round::new();
    let mut inputs = round.inputs(vec![round.ballot(&[4, 0], 1), round.ballot(&[0, 1], 2)]);
    inputs.ciphertexts[0].0 = vec![0xff; 32];

    let result = ComputeInput {
        fhe_inputs: inputs.clone(),
    }
    .process(fhe_processor)
    .expect("garbage must not abort the Secure Process");

    // Only the second ballot survives, so the tally is that ballot alone.
    let mut survivor = round.inputs_at(vec![round.ballot(&[0, 1], 2)], vec![Round::slot(1)]);
    survivor.ciphertexts[0].1 = 1;
    let reference = ComputeInput {
        fhe_inputs: survivor.clone(),
    }
    .process(fhe_processor)
    .unwrap();

    assert_eq!(result.ciphertext_hash, reference.ciphertext_hash);
    assert_eq!(
        round.decrypt_tally(&fhe_processor(&survivor), 2),
        vec![0, 1]
    );
}

/// Two provers over the same published data must agree, or the excluded set would be something a
/// prover chooses rather than something the inputs determine.
#[test]
fn the_result_is_deterministic_across_runs() {
    let round = Round::new();
    let mut inputs = round.inputs(vec![round.ballot(&[1, 0], 1), round.ballot(&[0, 2], 2)]);
    inputs.ciphertexts[0].0 = vec![0x00; 24];

    let first = ComputeInput {
        fhe_inputs: inputs.clone(),
    }
    .process(fhe_processor)
    .unwrap();
    let second = ComputeInput { fhe_inputs: inputs }
        .process(fhe_processor)
        .unwrap();

    assert_eq!(first.merkle_root, second.merkle_root);
    assert_eq!(first.ciphertext_hash, second.ciphertext_hash);
    assert_eq!(first.ciphertext_commitment, second.ciphertext_commitment);
}

/// Reordering the inputs changes the root, which is why the indexer must sort by the on-chain
/// index before handing them over.
#[test]
fn input_order_changes_the_root() {
    let round = Round::new();
    let ballots = vec![round.ballot(&[1, 0], 1), round.ballot(&[0, 2], 2)];

    let ordered = ComputeInput {
        fhe_inputs: round.inputs(ballots.clone()),
    }
    .process(fhe_processor)
    .unwrap();

    let swapped = ComputeInput {
        fhe_inputs: round.inputs(vec![ballots[1].clone(), ballots[0].clone()]),
    }
    .process(fhe_processor)
    .unwrap();

    assert_ne!(ordered.merkle_root, swapped.merkle_root);
}

/// With every input unusable the round has nothing to tally and fails with a typed error rather
/// than a panic. Only reachable when no honest input exists.
#[test]
fn an_all_unusable_round_fails_cleanly() {
    let round = Round::new();
    let mut inputs = round.inputs(vec![round.ballot(&[1, 0], 1)]);
    inputs.ciphertexts[0].0 = vec![0xab; 16];

    let error = ComputeInput { fhe_inputs: inputs }
        .process(fhe_processor)
        .unwrap_err();

    assert!(
        matches!(error, ComputeError::OutputCommitment(_)),
        "expected a typed error, got {error:?}"
    );
}

/// The mask-poisoning case, through the real Secure Process.
///
/// A third party appends an entry to a slot that already holds a counted vote, with a real
/// commitment and bytes that are not its ciphertext. Append-only means the earlier entry is still
/// in the tree, so the victim's vote survives instead of being erased.
#[test]
fn a_poisoned_append_does_not_erase_the_vote_already_in_the_slot() {
    let round = Round::new();
    let victim = Round::slot(7);
    let honest_ballot = round.ballot(&[6, 0], 1);

    let mut inputs = round.inputs_at(vec![honest_ballot.clone()], vec![victim]);
    // The attacker's entry: it reuses the victim's commitment beside unrelated bytes.
    inputs.ciphertexts.push((round.ballot(&[0, 9], 5), 1));
    inputs.commitments.push(inputs.commitments[0]);
    inputs.slots.push(victim);

    let result = ComputeInput { fhe_inputs: inputs }
        .process(fhe_processor)
        .expect("a poisoned append must not stop the round");

    // The tally is the victim's original ballot, untouched.
    let reference = round.inputs_at(vec![honest_ballot], vec![victim]);
    assert_eq!(
        round.decrypt_tally(&fhe_processor(&reference), 2),
        vec![6, 0]
    );

    let reference_result = ComputeInput {
        fhe_inputs: reference,
    }
    .process(fhe_processor)
    .unwrap();
    assert_eq!(
        result.ciphertext_hash, reference_result.ciphertext_hash,
        "the poisoned append must not change the tally"
    );
}

/// An honest re-vote still replaces the earlier ballot, so append-only does not freeze a voter's
/// first choice.
#[test]
fn an_honest_re_vote_replaces_the_earlier_ballot() {
    let round = Round::new();
    let voter = Round::slot(2);
    let first = round.ballot(&[1, 0], 1);
    let second = round.ballot(&[0, 7], 2);

    let mut inputs = round.inputs_at(vec![first], vec![voter]);
    let later = round.inputs_at(vec![second.clone()], vec![voter]);
    inputs.ciphertexts.push((second.clone(), 1));
    inputs.commitments.push(later.commitments[0]);
    inputs.slots.push(voter);

    let result = ComputeInput { fhe_inputs: inputs }
        .process(fhe_processor)
        .unwrap();

    let reference = round.inputs_at(vec![second], vec![voter]);
    assert_eq!(
        round.decrypt_tally(&fhe_processor(&reference), 2),
        vec![0, 7]
    );
    let reference_result = ComputeInput {
        fhe_inputs: reference,
    }
    .process(fhe_processor)
    .unwrap();
    assert_eq!(result.ciphertext_hash, reference_result.ciphertext_hash);
}
