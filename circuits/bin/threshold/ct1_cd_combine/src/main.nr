// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A combine, level 1 (P3, ct1 leg): verifies `ct1_pk_commit` and `ct1_ct_commit` (both
//! hold raw sensitive witness data, so both are ZK-verified) and passes their commitments
//! through.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};

fn main(
    c_vk: UltraHonkVerificationKey,
    c_proof: UltraHonkZKProof,
    c_public_inputs: [Field; 1],
    c_key_hash: pub Field,
    d_vk: UltraHonkVerificationKey,
    d_proof: UltraHonkZKProof,
    d_public_inputs: [Field; 1],
    d_key_hash: pub Field,
) -> pub (Field, Field) {
    verify_honk_proof(c_vk, c_proof, c_public_inputs, c_key_hash);
    verify_honk_proof(d_vk, d_proof, d_public_inputs, d_key_hash);

    (c_public_inputs[0], d_public_inputs[0])
}
