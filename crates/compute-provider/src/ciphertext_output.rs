// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::compute_input::ComputeInput;

pub trait ComputeProvider {
    type Output: Send + Sync;

    fn prove(&self, input: &ComputeInput) -> Self::Output;
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ComputeResult {
    pub ciphertext_hash: Vec<u8>,
    pub ciphertext_commitment: Vec<u8>,
    pub params_hash: Vec<u8>,
    pub merkle_root: Vec<u8>,
}
