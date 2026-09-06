// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A combine, level 1 (P3): verifies `ct0_e0_r2_check` and `ct0_r1_check` (both hold raw
//! sensitive witness data, so both are proven ZK and verified here via `verify_honk_proof`,
//! matching how DKG's `c2_chunk_batch` verifies its own raw-witness chunk proofs) and passes
//! their commitments through. Paired with `ct0_cd_combine` at the next level by
//! `ct0_abcd_combine`.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};

fn main(
    e0_r2_vk: UltraHonkVerificationKey,
    e0_r2_proof: UltraHonkZKProof,
    e0_r2_public_inputs: [Field; 2],
    e0_r2_key_hash: pub Field,
    r1_vk: UltraHonkVerificationKey,
    r1_proof: UltraHonkZKProof,
    r1_public_inputs: [Field; 1],
    r1_key_hash: pub Field,
) -> pub (Field, Field, Field) {
    verify_honk_proof(e0_r2_vk, e0_r2_proof, e0_r2_public_inputs, e0_r2_key_hash);
    verify_honk_proof(r1_vk, r1_proof, r1_public_inputs, r1_key_hash);

    (e0_r2_public_inputs[0], e0_r2_public_inputs[1], r1_public_inputs[0])
}
