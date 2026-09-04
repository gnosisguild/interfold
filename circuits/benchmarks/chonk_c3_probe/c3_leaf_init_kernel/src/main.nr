// SPDX-License-Identifier: LGPL-3.0-only

use ivc_types::{
    append_c3_leaf, C3AccumulatorState, C3LeafOutput, empty_c3_state, PROOF_TYPE_OINK,
    VerificationKey,
};

/// Initialize the running accumulator from the first leaf app.
fn main(
    app_inputs: call_data(1) C3LeafOutput,
    app_vk: VerificationKey,
) -> return_data C3AccumulatorState {
    std::verify_proof_with_type(app_vk.key, [], [], app_vk.hash, PROOF_TYPE_OINK);
    append_c3_leaf(empty_c3_state(), app_inputs)
}
