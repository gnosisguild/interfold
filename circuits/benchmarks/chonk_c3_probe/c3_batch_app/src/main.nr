// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use bb_proof_verification::{UltraHonkVerificationKey, UltraHonkZKProof, verify_honk_proof};
use ivc_types::{C3_ACCUMULATOR_LEN, C3_BATCH_SIZE, C3_SLOTS};

/// Verify a batch of real C3 leaf proofs and emit the sparse pk/msg/ct tail.
/// The six-field c3_fold prefix is added by the ordinary UltraHonk tube, since
/// its values describe the outer aggregation proof rather than this app.
fn main(
    verification_key: UltraHonkVerificationKey,
    key_hash: Field,
    proofs: [UltraHonkZKProof; C3_BATCH_SIZE],
    public_inputs: [[Field; 5]; C3_BATCH_SIZE],
    slot_indices: [u32; C3_BATCH_SIZE],
) -> return_data [Field; C3_ACCUMULATOR_LEN] {
    let mut seen: [bool; C3_SLOTS] = [false; C3_SLOTS];
    let mut output: [Field; C3_ACCUMULATOR_LEN] = [0; C3_ACCUMULATOR_LEN];

    for i in 0..C3_BATCH_SIZE {
        let slot = slot_indices[i];
        assert(slot < C3_SLOTS);
        assert(!seen[slot]);
        seen[slot] = true;

        verify_honk_proof(verification_key, proofs[i], public_inputs[i], key_hash);
        assert(public_inputs[i][2] == (slot / 2) as Field);
        assert(public_inputs[i][3] == (slot % 2) as Field);

        output[slot] = public_inputs[i][0];
        output[C3_SLOTS + slot] = public_inputs[i][1];
        output[(2 * C3_SLOTS) + slot] = public_inputs[i][4];
    }

    output
}
