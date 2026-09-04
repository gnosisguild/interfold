// SPDX-License-Identifier: LGPL-3.0-only

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};
use ivc_types::{C3_SLOTS, C3LeafOutput};
use lib::configs::default::dkg::L_THRESHOLD;

/// Verify exactly one real ShareEncryption-shaped C3 proof.
fn main(
    verification_key: UltraHonkVerificationKey,
    key_hash: Field,
    proof: UltraHonkZKProof,
    public_inputs: [Field; 5],
    slot_index: u32,
) -> return_data C3LeafOutput {
    assert(slot_index < C3_SLOTS);
    verify_honk_proof(verification_key, proof, public_inputs, key_hash);
    assert(public_inputs[2] == (slot_index / L_THRESHOLD) as Field);
    assert(public_inputs[3] == (slot_index % L_THRESHOLD) as Field);

    C3LeafOutput {
        slot: slot_index,
        pk: public_inputs[0],
        msg: public_inputs[1],
        ct: public_inputs[4],
    }
}
