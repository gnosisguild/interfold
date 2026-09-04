// SPDX-License-Identifier: LGPL-3.0-only

use ivc_types::{
    C3_ACCUMULATOR_LEN, c3_state_to_accumulator, C3AccumulatorState, PROOF_TYPE_HN_FINAL,
    VerificationKey,
};

/// Hide the leaf-folding chain and expose the normal C3 accumulator tail.
fn main(
    prev_kernel_inputs: call_data(0) C3AccumulatorState,
    kernel_vk: VerificationKey,
) -> pub [Field; C3_ACCUMULATOR_LEN] {
    std::verify_proof_with_type(kernel_vk.key, [], [], kernel_vk.hash, PROOF_TYPE_HN_FINAL);
    c3_state_to_accumulator(prev_kernel_inputs)
}
