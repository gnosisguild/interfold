// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A combine, level 1 (P3, ct1 leg): verifies `ct1_e1_p2_u_check` and `ct1_p1_check` (both
//! hold raw sensitive witness data, so both are ZK-verified) and passes their commitments
//! through.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};

fn main(
    a_vk: UltraHonkVerificationKey,
    a_proof: UltraHonkZKProof,
    a_public_inputs: [Field; 3],
    a_key_hash: pub Field,
    b_vk: UltraHonkVerificationKey,
    b_proof: UltraHonkZKProof,
    b_public_inputs: [Field; 1],
    b_key_hash: pub Field,
) -> pub (Field, Field, Field, Field) {
    verify_honk_proof(a_vk, a_proof, a_public_inputs, a_key_hash);
    verify_honk_proof(b_vk, b_proof, b_public_inputs, b_key_hash);

    (a_public_inputs[0], a_public_inputs[1], a_public_inputs[2], b_public_inputs[0])
}
