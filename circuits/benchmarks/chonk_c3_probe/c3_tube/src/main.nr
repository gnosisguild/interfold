use ivc_types::{
    C3_ACCUMULATOR_LEN, C3_FOLD_PREFIX_LEN, C3_FOLD_PUBLIC_LEN, CHONK_PROOF_LENGTH,
    CHONK_VK_LENGTH_IN_FIELDS, PROOF_TYPE_CHONK,
};

/// Verify the Chonk proof and expose its accumulator tail through the exact
/// c3_fold public-input width expected by c3ab_fold/NodeFold adapters.
fn main(
    verification_key: [Field; CHONK_VK_LENGTH_IN_FIELDS],
    proof: [Field; CHONK_PROOF_LENGTH],
    chonk_public_inputs: [Field; C3_ACCUMULATOR_LEN],
    key_hash: Field,
    c3_public_inputs: pub [Field; C3_FOLD_PUBLIC_LEN],
) {
    std::verify_proof_with_type(
        verification_key,
        proof,
        chonk_public_inputs,
        key_hash,
        PROOF_TYPE_CHONK,
    );

    for i in 0..C3_ACCUMULATOR_LEN {
        assert(c3_public_inputs[C3_FOLD_PREFIX_LEN + i] == chonk_public_inputs[i]);
    }
}
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
