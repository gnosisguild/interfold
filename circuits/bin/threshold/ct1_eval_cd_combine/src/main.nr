// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B combine, level 1 (P3, ct1 leg): verifies `ct1_eval_pk` and `ct1_eval_ct` (both hold raw
//! sensitive witness data, so both are ZK-verified), checks both leaves used the same `gamma`,
//! and passes everything through.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};
use lib::configs::default::threshold::L;

global PK_CT_LEN: u32 = 2 + L;

fn main(
    pk_vk: UltraHonkVerificationKey,
    pk_proof: UltraHonkZKProof,
    pk_public_inputs: [Field; PK_CT_LEN],
    pk_key_hash: pub Field,
    ct_vk: UltraHonkVerificationKey,
    ct_proof: UltraHonkZKProof,
    ct_public_inputs: [Field; PK_CT_LEN],
    ct_key_hash: pub Field,
) -> pub (Field, Field, [Field; L], Field, [Field; L]) {
    verify_honk_proof(pk_vk, pk_proof, pk_public_inputs, pk_key_hash);
    verify_honk_proof(ct_vk, ct_proof, ct_public_inputs, ct_key_hash);

    assert(pk_public_inputs[0] == ct_public_inputs[0], "gamma mismatch");
    let gamma = pk_public_inputs[0];

    let pk1_commitment = pk_public_inputs[1];
    let mut pk1is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        pk1is_at_gamma[i] = pk_public_inputs[2 + i];
    }

    let ct1_commitment = ct_public_inputs[1];
    let mut ct1is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        ct1is_at_gamma[i] = ct_public_inputs[2 + i];
    }

    (gamma, pk1_commitment, pk1is_at_gamma, ct1_commitment, ct1is_at_gamma)
}
