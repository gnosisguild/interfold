// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B combine, level 3 - final (P3): verifies `ct0_eval_abcd_combine` (non-ZK) and
//! `ct0_eval_u_k1` (ZK, raw witness), then performs the actual ct0 Schwartz-Zippel identity check
//! - this is the one place all five witness groups' evaluations are simultaneously available, so
//! it's also the only place the cross-group products (`pk0is_at_gamma * u_at_gamma`,
//! `k1_at_gamma * k0is[i]`, etc.) can be formed. Replicates the monolithic circuit's
//! `verify_evaluations` exactly.
//!
//! `ct0_gammas` (the batching coefficients from Round A's challenge, `gammas[0..L]`) is taken
//! here as a plain input, not verified against Round A within this circuit - the top-level
//! `user_data_encryption_ct0` circuit verifies both this proof and Round A's `ct0_abcde_combine`
//! and checks they agree, closing that loop.

use bb_proof_verification::{
    UltraHonkProof, UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof,
    verify_honk_proof_non_zk,
};
use lib::configs::default::threshold::{L, N, QIS, USER_DATA_ENCRYPTION_K0IS};

global ABCD_LEN: u32 = 7 + (5 * L);
global E_LEN: u32 = 5;

fn main(
    abcd_vk: UltraHonkVerificationKey,
    abcd_proof: UltraHonkProof,
    abcd_public_inputs: [Field; ABCD_LEN],
    abcd_key_hash: pub Field,
    e_vk: UltraHonkVerificationKey,
    e_proof: UltraHonkZKProof,
    e_public_inputs: [Field; E_LEN],
    e_key_hash: pub Field,
    ct0_gammas: pub [Field; L],
) -> pub (Field, Field, Field, Field, Field, Field, Field, Field) {
    verify_honk_proof_non_zk(abcd_vk, abcd_proof, abcd_public_inputs, abcd_key_hash);
    verify_honk_proof(e_vk, e_proof, e_public_inputs, e_key_hash);

    let gamma = abcd_public_inputs[0];
    assert(gamma == e_public_inputs[0], "gamma mismatch");
    assert(gamma == ct0_gammas[0], "gamma mismatch with batching coefficients");

    let e0_commit = abcd_public_inputs[1];
    let r2_commit = abcd_public_inputs[2];
    let mut e0is_at_gamma: [Field; L] = [0; L];
    let mut r2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        e0is_at_gamma[i] = abcd_public_inputs[3 + i];
        r2i_at_gamma[i] = abcd_public_inputs[3 + L + i];
    }
    let r1_commit = abcd_public_inputs[3 + (2 * L)];
    let mut r1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        r1i_at_gamma[i] = abcd_public_inputs[4 + (2 * L) + i];
    }
    let pk0_commitment = abcd_public_inputs[4 + (3 * L)];
    let mut pk0is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        pk0is_at_gamma[i] = abcd_public_inputs[5 + (3 * L) + i];
    }
    let ct0_commitment = abcd_public_inputs[5 + (4 * L)];
    let mut ct0is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        ct0is_at_gamma[i] = abcd_public_inputs[6 + (4 * L) + i];
    }

    let u_commitment = e_public_inputs[1];
    let k1_commitment = e_public_inputs[2];
    let u_at_gamma = e_public_inputs[3];
    let k1_at_gamma = e_public_inputs[4];

    let cyclo_at_gamma = gamma.pow_32(N as Field) + 1;
    let mut sum = (0, 0);
    for i in 0..L {
        let mut ct0_rhs = pk0is_at_gamma[i] * u_at_gamma + e0is_at_gamma[i];
        ct0_rhs += k1_at_gamma * USER_DATA_ENCRYPTION_K0IS[i];
        ct0_rhs += r1i_at_gamma[i] * QIS[i];
        ct0_rhs += r2i_at_gamma[i] * cyclo_at_gamma;

        let ct0_lhs = ct0is_at_gamma[i];
        let gamma_i = if i == 0 { 1 } else { ct0_gammas[i] };

        sum = (sum.0 + ct0_lhs * gamma_i, sum.1 + ct0_rhs * gamma_i);
    }
    assert(sum.0 == sum.1, "ct0 encryption check failed");

    (
        pk0_commitment, ct0_commitment, k1_commitment, u_commitment, e0_commit, r2_commit,
        r1_commit, gamma,
    )
}
