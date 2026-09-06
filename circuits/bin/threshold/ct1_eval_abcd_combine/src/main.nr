// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B combine, level 2 - final (P3, ct1 leg): verifies `ct1_eval_ab_combine` and
//! `ct1_eval_cd_combine` (both non-ZK), then performs the actual ct1 Schwartz-Zippel identity
//! check - the one place all four witness groups' evaluations are simultaneously available.
//! Replicates the monolithic circuit's `verify_evaluations` for ct1 exactly, including its
//! difference from ct0: the batching coefficient is `ct1_batching_coeffs[i]` for every limb `i`,
//! with no `i == 0` special-case.
//!
//! `ct1_batching_coeffs` (`gammas[L..2L]` from Round A's challenge) is taken as a plain input,
//! not verified against Round A here - the top-level `user_data_encryption_ct1` circuit verifies
//! both this proof and Round A's `ct1_abcd_combine` and checks they agree.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::threshold::{L, N, QIS};

global AB_LEN: u32 = 9 + (2 * L);
global CD_LEN: u32 = 5 + (2 * L);

fn main(
    ab_vk: UltraHonkVerificationKey,
    ab_proof: UltraHonkProof,
    ab_public_inputs: [Field; AB_LEN],
    ab_key_hash: pub Field,
    cd_vk: UltraHonkVerificationKey,
    cd_proof: UltraHonkProof,
    cd_public_inputs: [Field; CD_LEN],
    cd_key_hash: pub Field,
    ct1_batching_coeffs: pub [Field; L],
) -> pub (Field, Field, Field, Field, Field, Field, Field) {
    verify_honk_proof_non_zk(ab_vk, ab_proof, ab_public_inputs, ab_key_hash);
    verify_honk_proof_non_zk(cd_vk, cd_proof, cd_public_inputs, cd_key_hash);

    let gamma = ab_public_inputs[0];
    assert(gamma == cd_public_inputs[0], "gamma mismatch");

    let e1_commit = ab_public_inputs[1];
    let p2_commit = ab_public_inputs[2];
    let u_commitment = ab_public_inputs[3];
    let e1_at_gamma = ab_public_inputs[4];
    let mut p2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        p2i_at_gamma[i] = ab_public_inputs[5 + i];
    }
    let u_at_gamma = ab_public_inputs[5 + L];
    let p1_commit = ab_public_inputs[6 + L];
    let mut p1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        p1i_at_gamma[i] = ab_public_inputs[7 + L + i];
    }

    let pk1_commitment = cd_public_inputs[1];
    let mut pk1is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        pk1is_at_gamma[i] = cd_public_inputs[2 + i];
    }
    let ct1_commitment = cd_public_inputs[2 + L];
    let mut ct1is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        ct1is_at_gamma[i] = cd_public_inputs[3 + L + i];
    }

    let cyclo_at_gamma = gamma.pow_32(N as Field) + 1;
    let mut sum = (0, 0);
    for i in 0..L {
        let mut ct1_rhs = pk1is_at_gamma[i] * u_at_gamma + e1_at_gamma;
        ct1_rhs += p1i_at_gamma[i] * QIS[i];
        ct1_rhs += p2i_at_gamma[i] * cyclo_at_gamma;

        let ct1_lhs = ct1is_at_gamma[i];
        let gamma_i = ct1_batching_coeffs[i];

        sum = (sum.0 + ct1_lhs * gamma_i, sum.1 + ct1_rhs * gamma_i);
    }
    assert(sum.0 == sum.1, "ct1 encryption check failed");

    (pk1_commitment, ct1_commitment, u_commitment, e1_commit, p2_commit, p1_commit, gamma)
}
