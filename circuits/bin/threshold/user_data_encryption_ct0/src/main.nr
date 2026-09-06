// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Top-level ct0 (P3): the monolithic `UserDataEncryptionCt0` circuit no longer fits a single
//! browser-side proof at N=16384, L=5, so its work is split across five witness-group circuits
//! (e0+r2is, r1is, pk0is, ct0is, u+k1), each range-checked/committed in Round A
//! (`ct0_*_check`/`ct0_pk_commit`/`ct0_ct_commit`, combined via `ct0_ab_combine` -> `ct0_cd_combine`
//! -> `ct0_abcd_combine` -> `ct0_abcde_combine`, which also derives `gamma`) and re-evaluated at
//! `gamma` in Round B (`ct0_eval_*`, combined the same way via `ct0_eval_ab_combine` ->
//! `ct0_eval_cd_combine` -> `ct0_eval_abcd_combine` -> `ct0_eval_abcde_combine`, which performs the
//! actual Schwartz-Zippel identity check).
//!
//! This circuit is where those two chains finally meet: it verifies both final-combine proofs and
//! checks every commitment and every batching coefficient they produced actually agree - closing
//! the binding that stops a prover from range-checking one witness in Round A and evaluating a
//! different, unconstrained one in Round B. Output shape is unchanged from the monolithic
//! circuit, so the `user_data_encryption` wrapper needs no changes.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::threshold::L;

global ROUND_A_LEN: u32 = 9 + L;
global ROUND_B_LEN: u32 = 10 + L;

fn main(
    a_vk: UltraHonkVerificationKey,
    a_proof: UltraHonkProof,
    a_public_inputs: [Field; ROUND_A_LEN],
    a_key_hash: pub Field,
    b_vk: UltraHonkVerificationKey,
    b_proof: UltraHonkProof,
    b_public_inputs: [Field; ROUND_B_LEN],
    b_key_hash: pub Field,
) -> pub (Field, Field, Field, Field) {
    verify_honk_proof_non_zk(a_vk, a_proof, a_public_inputs, a_key_hash);
    verify_honk_proof_non_zk(b_vk, b_proof, b_public_inputs, b_key_hash);

    // Round A (`ct0_abcde_combine`): 2 key-hash params, then 7 commitments + ct0_gammas[L].
    let a_pk0_commitment = a_public_inputs[2];
    let a_ct0_commitment = a_public_inputs[3];
    let a_k1_commitment = a_public_inputs[4];
    let a_u_commitment = a_public_inputs[5];
    let a_e0_commit = a_public_inputs[6];
    let a_r2_commit = a_public_inputs[7];
    let a_r1_commit = a_public_inputs[8];
    let mut a_gammas: [Field; L] = [0; L];
    for i in 0..L {
        a_gammas[i] = a_public_inputs[9 + i];
    }

    // Round B (`ct0_eval_abcde_combine`): 2 key-hash params + ct0_gammas[L] param, then 8 outputs.
    let mut b_gammas: [Field; L] = [0; L];
    for i in 0..L {
        b_gammas[i] = b_public_inputs[2 + i];
    }
    let b_ret_start = 2 + L;
    let b_pk0_commitment = b_public_inputs[b_ret_start];
    let b_ct0_commitment = b_public_inputs[b_ret_start + 1];
    let b_k1_commitment = b_public_inputs[b_ret_start + 2];
    let b_u_commitment = b_public_inputs[b_ret_start + 3];
    let b_e0_commit = b_public_inputs[b_ret_start + 4];
    let b_r2_commit = b_public_inputs[b_ret_start + 5];
    let b_r1_commit = b_public_inputs[b_ret_start + 6];
    let b_gamma = b_public_inputs[b_ret_start + 7];

    assert(a_pk0_commitment == b_pk0_commitment, "pk0_commitment mismatch between rounds");
    assert(a_ct0_commitment == b_ct0_commitment, "ct0_commitment mismatch between rounds");
    assert(a_k1_commitment == b_k1_commitment, "k1_commitment mismatch between rounds");
    assert(a_u_commitment == b_u_commitment, "u_commitment mismatch between rounds");
    assert(a_e0_commit == b_e0_commit, "e0 commitment mismatch between rounds");
    assert(a_r2_commit == b_r2_commit, "r2 commitment mismatch between rounds");
    assert(a_r1_commit == b_r1_commit, "r1 commitment mismatch between rounds");
    assert(a_gammas[0] == b_gamma, "gamma mismatch between rounds");
    for i in 0..L {
        assert(a_gammas[i] == b_gammas[i], "batching-coefficient mismatch between rounds");
    }

    (a_pk0_commitment, a_ct0_commitment, a_k1_commitment, a_u_commitment)
}
