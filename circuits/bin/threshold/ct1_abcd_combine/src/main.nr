// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A combine, level 2 - final (P3, ct1 leg): verifies `ct1_ab_combine` and `ct1_cd_combine`
//! (both non-ZK, already aggregated), gathers every commitment the ct1 challenge depends on, then
//! derives `gamma`. Same replacement as ct0's `ct0_abcde_combine`: the transcript hashes
//! commitments instead of raw `e1`/`p1is`/`p2is` coefficients.
//!
//! Unlike ct0, ct1's batching coefficients are `gammas[L..2L]` for every limb `i` (no `i == 0`
//! special-case) - see `verify_evaluations` in the original `UserDataEncryptionCt1`.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::threshold::L;
use lib::math::commitments::compute_user_data_encryption_ct1_challenge;

fn main(
    ab_vk: UltraHonkVerificationKey,
    ab_proof: UltraHonkProof,
    ab_public_inputs: [Field; 4],
    ab_key_hash: pub Field,
    cd_vk: UltraHonkVerificationKey,
    cd_proof: UltraHonkProof,
    cd_public_inputs: [Field; 2],
    cd_key_hash: pub Field,
) -> pub (Field, Field, Field, Field, Field, Field, Field, [Field; L]) {
    verify_honk_proof_non_zk(ab_vk, ab_proof, ab_public_inputs, ab_key_hash);
    verify_honk_proof_non_zk(cd_vk, cd_proof, cd_public_inputs, cd_key_hash);

    let e1_commit = ab_public_inputs[0];
    let p2_commit = ab_public_inputs[1];
    let u_commitment = ab_public_inputs[2];
    let p1_commit = ab_public_inputs[3];
    let pk1_commitment = cd_public_inputs[0];
    let ct1_commitment = cd_public_inputs[1];

    let payload =
        [pk1_commitment, ct1_commitment, u_commitment, e1_commit, p1_commit, p2_commit].as_vector();
    let gammas = compute_user_data_encryption_ct1_challenge::<L>(payload);

    let gamma = gammas[0];
    let mut ct1_batching_coeffs: [Field; L] = [0; L];
    for i in 0..L {
        ct1_batching_coeffs[i] = gammas[L + i];
    }

    (
        pk1_commitment, ct1_commitment, u_commitment, e1_commit, p2_commit, p1_commit, gamma,
        ct1_batching_coeffs,
    )
}
