// SPDX-License-Identifier: LGPL-3.0-only

use ivc_types::{append_c3_leaf, C3AccumulatorState, C3LeafOutput, PROOF_TYPE_HN, VerificationKey};

/// Fold one leaf app into the running accumulator.
fn main(
    prev_kernel_inputs: call_data(0) C3AccumulatorState,
    kernel_vk: VerificationKey,
    app_inputs: call_data(1) C3LeafOutput,
    app_vk: VerificationKey,
) -> return_data C3AccumulatorState {
    std::verify_proof_with_type(kernel_vk.key, [], [], kernel_vk.hash, PROOF_TYPE_HN);
    std::verify_proof_with_type(app_vk.key, [], [], app_vk.hash, PROOF_TYPE_HN);
    append_c3_leaf(prev_kernel_inputs, app_inputs)
}
