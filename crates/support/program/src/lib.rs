// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use e3_compute_provider::{FHEInputs, InputPolicy};
use e3_fhe_params::decode_bfv_params_arc;
use fhe::bfv::Ciphertext;
use fhe_traits::{DeserializeParametrized, Serialize};

/// CRISP Implementation of the CiphertextProcessor function
pub fn fhe_processor(fhe_inputs: &FHEInputs) -> Vec<u8> {
    let params = decode_bfv_params_arc(&fhe_inputs.params).expect("Failed to decode BFV params");

    let mut sum = Ciphertext::zero(&params);
    for ciphertext_bytes in &fhe_inputs.ciphertexts {
        let ciphertext = Ciphertext::from_bytes(&ciphertext_bytes.0, &params).unwrap();
        sum += &ciphertext;
    }

    sum.to_bytes()
}

/// How the Secure Process builds input-tree leaves and chooses which inputs to compute over.
///
/// The default is the behaviour every E3 program had before policies existed: the leaf is the
/// ciphertext's own commitment, and every input is computed over. It is the right starting point
/// for a program whose contract inserts the commitment directly, as this template's does.
///
/// A program that publishes more than the ciphertext — a slot, a parent, anything the contract
/// folds into its leaf — must return a policy that rebuilds the *same* leaf here, or the root the
/// guest derives will not match the one the contract built and the round cannot publish. See
/// CRISP's `policy()` for a worked example.
pub fn policy() -> InputPolicy {
    InputPolicy::default()
}
