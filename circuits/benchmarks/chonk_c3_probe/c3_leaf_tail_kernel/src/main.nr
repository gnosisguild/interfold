// SPDX-License-Identifier: LGPL-3.0-only

use ivc_types::{C3AccumulatorState, PROOF_TYPE_HN_TAIL, VerificationKey};

/// Close the last leaf kernel before the hiding kernel.
fn main(
    prev_kernel_inputs: call_data(0) C3AccumulatorState,
    kernel_vk: VerificationKey,
) -> return_data C3AccumulatorState {
    std::verify_proof_with_type(kernel_vk.key, [], [], kernel_vk.hash, PROOF_TYPE_HN_TAIL);
    prev_kernel_inputs
}
