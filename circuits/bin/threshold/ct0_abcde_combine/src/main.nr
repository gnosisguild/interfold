// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A combine, level 3 - final (P3): verifies `ct0_abcd_combine` (non-ZK, already
//! aggregated) and `ct0_u_k1_check` (ZK, raw witness), gathering every commitment the ct0
//! challenge depends on, then derives `gamma`.
//!
//! This replaces the monolithic circuit's `generate_challenge`, which hashed raw `e0`/`r1is`/
//! `r2is` coefficients directly into the transcript. That's no longer possible once those
//! witnesses live in separate proofs - instead the transcript hashes their *commitments*
//! (`e0_commit`/`r2_commit`/`r1_commit`), which is safe because `gamma`'s exact derivation isn't
//! a cross-circuit protocol invariant (unlike `pk0_commitment`/`ct0_commitment`) - it only needs
//! to be self-consistent between ct0's own Round A and Round B.

use bb_proof_verification::{
    UltraHonkProof, UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof,
    verify_honk_proof_non_zk,
};
use lib::configs::default::threshold::L;
use lib::math::commitments::compute_user_data_encryption_ct0_challenge;

fn main(
    abcd_vk: UltraHonkVerificationKey,
    abcd_proof: UltraHonkProof,
    abcd_public_inputs: [Field; 5],
    abcd_key_hash: pub Field,
    e_vk: UltraHonkVerificationKey,
    e_proof: UltraHonkZKProof,
    e_public_inputs: [Field; 2],
    e_key_hash: pub Field,
) -> pub (Field, Field, Field, Field, Field, Field, Field, [Field; L]) {
    verify_honk_proof_non_zk(abcd_vk, abcd_proof, abcd_public_inputs, abcd_key_hash);
    verify_honk_proof(e_vk, e_proof, e_public_inputs, e_key_hash);

    let e0_commit = abcd_public_inputs[0];
    let r2_commit = abcd_public_inputs[1];
    let r1_commit = abcd_public_inputs[2];
    let pk0_commitment = abcd_public_inputs[3];
    let ct0_commitment = abcd_public_inputs[4];
    let u_commitment = e_public_inputs[0];
    let k1_commitment = e_public_inputs[1];

    let payload = [
        pk0_commitment,
        ct0_commitment,
        k1_commitment,
        u_commitment,
        e0_commit,
        r2_commit,
        r1_commit,
    ]
        .as_vector();
    let gammas = compute_user_data_encryption_ct0_challenge::<L>(payload);

    let mut ct0_gammas: [Field; L] = [0; L];
    for i in 0..L {
        ct0_gammas[i] = gammas[i];
    }

    (
        pk0_commitment, ct0_commitment, k1_commitment, u_commitment, e0_commit, r2_commit,
        r1_commit, ct0_gammas,
    )
}
