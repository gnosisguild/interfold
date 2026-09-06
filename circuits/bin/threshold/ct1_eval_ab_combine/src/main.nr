// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B combine, level 1 (P3, ct1 leg): verifies `ct1_eval_e1_p2_u` and `ct1_eval_p1` (both
//! hold raw sensitive witness data, so both are ZK-verified), checks both leaves used the same
//! `gamma`, and passes everything through.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};
use lib::configs::default::threshold::L;

global A_LEN: u32 = 6 + L;
global B_LEN: u32 = 2 + L;

fn main(
    a_vk: UltraHonkVerificationKey,
    a_proof: UltraHonkZKProof,
    a_public_inputs: [Field; A_LEN],
    a_key_hash: pub Field,
    b_vk: UltraHonkVerificationKey,
    b_proof: UltraHonkZKProof,
    b_public_inputs: [Field; B_LEN],
    b_key_hash: pub Field,
) -> pub (Field, Field, Field, Field, Field, [Field; L], Field, Field, [Field; L]) {
    verify_honk_proof(a_vk, a_proof, a_public_inputs, a_key_hash);
    verify_honk_proof(b_vk, b_proof, b_public_inputs, b_key_hash);

    assert(a_public_inputs[0] == b_public_inputs[0], "gamma mismatch");
    let gamma = a_public_inputs[0];

    let e1_commit = a_public_inputs[1];
    let p2_commit = a_public_inputs[2];
    let u_commitment = a_public_inputs[3];
    let e1_at_gamma = a_public_inputs[4];
    let mut p2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        p2i_at_gamma[i] = a_public_inputs[5 + i];
    }
    let u_at_gamma = a_public_inputs[5 + L];

    let p1_commit = b_public_inputs[1];
    let mut p1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        p1i_at_gamma[i] = b_public_inputs[2 + i];
    }

    (
        gamma, e1_commit, p2_commit, u_commitment, e1_at_gamma, p2i_at_gamma, u_at_gamma, p1_commit,
        p1i_at_gamma,
    )
}
