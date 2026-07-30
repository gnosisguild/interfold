// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pub mod circuit;
pub mod codegen;
pub mod computation;
pub mod sample;
pub mod utils;

pub use circuit::{ShareComputationCircuit, ShareComputationCircuitData};
pub use computation::{
    batch_count, chunk_count, chunks_per_batch, Bits, Bounds, ChunkInputs, Configs, Inputs,
    ShareComputationOutput, SHARE_COMPUTATION_CHUNK_SIZE,
};
pub use sample::SecretShares;
