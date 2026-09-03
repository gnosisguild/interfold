// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use ivc_types::{C3_ACCUMULATOR_LEN, PROOF_TYPE_OINK, VerificationKey};

fn main(
    app_inputs: call_data(1) [Field; C3_ACCUMULATOR_LEN],
    app_vk: VerificationKey,
) -> return_data [Field; C3_ACCUMULATOR_LEN] {
    std::verify_proof_with_type(app_vk.key, [], [], app_vk.hash, PROOF_TYPE_OINK);
    app_inputs
}
