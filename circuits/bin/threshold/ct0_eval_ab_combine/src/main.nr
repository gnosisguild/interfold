// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B combine, level 1 (P3): verifies `ct0_eval_e0_r2` and `ct0_eval_r1` (both hold raw
//! sensitive witness data, so both are ZK-verified), checks both leaves used the same `gamma`
//! (public input index 0 in each - `gamma` is the leaves' only `pub` parameter, so it lands first
//! in the ACIR public-input layout, ahead of the returned values), and passes everything through.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};
use lib::configs::default::threshold::L;

global E0_R2_LEN: u32 = 3 + (2 * L);
global R1_LEN: u32 = 2 + L;

fn main(
    e0_r2_vk: UltraHonkVerificationKey,
    e0_r2_proof: UltraHonkZKProof,
    e0_r2_public_inputs: [Field; E0_R2_LEN],
    e0_r2_key_hash: pub Field,
    r1_vk: UltraHonkVerificationKey,
    r1_proof: UltraHonkZKProof,
    r1_public_inputs: [Field; R1_LEN],
    r1_key_hash: pub Field,
) -> pub (Field, Field, Field, [Field; L], [Field; L], Field, [Field; L]) {
    verify_honk_proof(e0_r2_vk, e0_r2_proof, e0_r2_public_inputs, e0_r2_key_hash);
    verify_honk_proof(r1_vk, r1_proof, r1_public_inputs, r1_key_hash);

    assert(e0_r2_public_inputs[0] == r1_public_inputs[0], "gamma mismatch");
    let gamma = e0_r2_public_inputs[0];

    let e0_commit = e0_r2_public_inputs[1];
    let r2_commit = e0_r2_public_inputs[2];
    let mut e0is_at_gamma: [Field; L] = [0; L];
    let mut r2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        e0is_at_gamma[i] = e0_r2_public_inputs[3 + i];
        r2i_at_gamma[i] = e0_r2_public_inputs[3 + L + i];
    }

    let r1_commit = r1_public_inputs[1];
    let mut r1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        r1i_at_gamma[i] = r1_public_inputs[2 + i];
    }

    (gamma, e0_commit, r2_commit, e0is_at_gamma, r2i_at_gamma, r1_commit, r1i_at_gamma)
}
