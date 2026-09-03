// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use ivc_types::{C3_ACCUMULATOR_LEN, PROOF_TYPE_HN_FINAL, VerificationKey};

fn main(
    prev_kernel_inputs: call_data(0) [Field; C3_ACCUMULATOR_LEN],
    kernel_vk: VerificationKey,
) -> pub [Field; C3_ACCUMULATOR_LEN] {
    std::verify_proof_with_type(kernel_vk.key, [], [], kernel_vk.hash, PROOF_TYPE_HN_FINAL);
    prev_kernel_inputs
}
