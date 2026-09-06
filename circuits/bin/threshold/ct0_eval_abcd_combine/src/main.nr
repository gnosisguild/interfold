// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B combine, level 2 (P3): verifies `ct0_eval_ab_combine` and `ct0_eval_cd_combine`. Both
//! inputs are themselves already-aggregated, non-sensitive values, so both are non-ZK-verified,
//! matching Round A's `ct0_abcd_combine`. Checks both sides used the same `gamma` and passes
//! everything through.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::threshold::L;

global AB_LEN: u32 = 4 + (3 * L);
global CD_LEN: u32 = 3 + (2 * L);

fn main(
    ab_vk: UltraHonkVerificationKey,
    ab_proof: UltraHonkProof,
    ab_public_inputs: [Field; AB_LEN],
    ab_key_hash: pub Field,
    cd_vk: UltraHonkVerificationKey,
    cd_proof: UltraHonkProof,
    cd_public_inputs: [Field; CD_LEN],
    cd_key_hash: pub Field,
    ) -> pub (Field, Field, Field, [Field; L], [Field; L], Field, [Field; L], Field, [Field; L], Field, [Field; L]) {
    verify_honk_proof_non_zk(ab_vk, ab_proof, ab_public_inputs, ab_key_hash);
    verify_honk_proof_non_zk(cd_vk, cd_proof, cd_public_inputs, cd_key_hash);

    assert(ab_public_inputs[0] == cd_public_inputs[0], "gamma mismatch");
    let gamma = ab_public_inputs[0];

    let e0_commit = ab_public_inputs[1];
    let r2_commit = ab_public_inputs[2];
    let mut e0is_at_gamma: [Field; L] = [0; L];
    let mut r2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        e0is_at_gamma[i] = ab_public_inputs[3 + i];
        r2i_at_gamma[i] = ab_public_inputs[3 + L + i];
    }
    let r1_commit = ab_public_inputs[3 + (2 * L)];
    let mut r1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        r1i_at_gamma[i] = ab_public_inputs[4 + (2 * L) + i];
    }

    let pk0_commitment = cd_public_inputs[1];
    let mut pk0is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        pk0is_at_gamma[i] = cd_public_inputs[2 + i];
    }
    let ct0_commitment = cd_public_inputs[2 + L];
    let mut ct0is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        ct0is_at_gamma[i] = cd_public_inputs[3 + L + i];
    }

    (
        gamma, e0_commit, r2_commit, e0is_at_gamma, r2i_at_gamma, r1_commit, r1i_at_gamma,
        pk0_commitment, pk0is_at_gamma, ct0_commitment, ct0is_at_gamma,
    )
}
