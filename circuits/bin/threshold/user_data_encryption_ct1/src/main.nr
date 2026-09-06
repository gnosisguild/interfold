// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Top-level ct1 (P3): mirrors ct0's split - the monolithic `UserDataEncryptionCt1` circuit no
//! longer fits a single browser-side proof, so its work is split across four witness-group
//! circuits (e1+p2is+u, p1is, pk1is, ct1is), checked in Round A (`ct1_*_check`/`ct1_pk_commit`/
//! `ct1_ct_commit`, combined via `ct1_ab_combine` -> `ct1_cd_combine` -> `ct1_abcd_combine`, which
//! also derives `gamma`) and re-evaluated at `gamma` in Round B (`ct1_eval_*`, combined via
//! `ct1_eval_ab_combine` -> `ct1_eval_cd_combine` -> `ct1_eval_abcd_combine`, which performs the
//! actual Schwartz-Zippel identity check).
//!
//! This circuit verifies both final-combine proofs and checks every commitment and batching
//! coefficient they produced agree. Output shape is unchanged, so the `user_data_encryption`
//! wrapper needs no changes.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::threshold::L;

global ROUND_LEN: u32 = 9 + L;

fn main(
    a_vk: UltraHonkVerificationKey,
    a_proof: UltraHonkProof,
    a_public_inputs: [Field; ROUND_LEN],
    a_key_hash: pub Field,
    b_vk: UltraHonkVerificationKey,
    b_proof: UltraHonkProof,
    b_public_inputs: [Field; ROUND_LEN],
    b_key_hash: pub Field,
) -> pub (Field, Field, Field) {
    verify_honk_proof_non_zk(a_vk, a_proof, a_public_inputs, a_key_hash);
    verify_honk_proof_non_zk(b_vk, b_proof, b_public_inputs, b_key_hash);

    // Round A (`ct1_abcd_combine`): 2 key-hash params, then 6 commitments + gamma + batching[L].
    let a_pk1_commitment = a_public_inputs[2];
    let a_ct1_commitment = a_public_inputs[3];
    let a_u_commitment = a_public_inputs[4];
    let a_e1_commit = a_public_inputs[5];
    let a_p2_commit = a_public_inputs[6];
    let a_p1_commit = a_public_inputs[7];
    let a_gamma = a_public_inputs[8];
    let mut a_batching: [Field; L] = [0; L];
    for i in 0..L {
        a_batching[i] = a_public_inputs[9 + i];
    }

    // Round B (`ct1_eval_abcd_combine`): 2 key-hash params + batching[L] param, then 7 outputs.
    let mut b_batching: [Field; L] = [0; L];
    for i in 0..L {
        b_batching[i] = b_public_inputs[2 + i];
    }
    let b_ret_start = 2 + L;
    let b_pk1_commitment = b_public_inputs[b_ret_start];
    let b_ct1_commitment = b_public_inputs[b_ret_start + 1];
    let b_u_commitment = b_public_inputs[b_ret_start + 2];
    let b_e1_commit = b_public_inputs[b_ret_start + 3];
    let b_p2_commit = b_public_inputs[b_ret_start + 4];
    let b_p1_commit = b_public_inputs[b_ret_start + 5];
    let b_gamma = b_public_inputs[b_ret_start + 6];

    assert(a_pk1_commitment == b_pk1_commitment, "pk1_commitment mismatch between rounds");
    assert(a_ct1_commitment == b_ct1_commitment, "ct1_commitment mismatch between rounds");
    assert(a_u_commitment == b_u_commitment, "u_commitment mismatch between rounds");
    assert(a_e1_commit == b_e1_commit, "e1 commitment mismatch between rounds");
    assert(a_p2_commit == b_p2_commit, "p2 commitment mismatch between rounds");
    assert(a_p1_commit == b_p1_commit, "p1 commitment mismatch between rounds");
    assert(a_gamma == b_gamma, "gamma mismatch between rounds");
    for i in 0..L {
        assert(a_batching[i] == b_batching[i], "batching-coefficient mismatch between rounds");
    }

    (a_pk1_commitment, a_ct1_commitment, a_u_commitment)
}
