// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Share-decryption circuit: proves correct decryption of authorized parties' ciphertexts under the DKG secret key (CIRCUIT 4a/4b).

pub mod circuit;
pub mod codegen;
pub mod computation;
pub mod sample;
pub use circuit::{ShareDecryptionCircuit, ShareDecryptionCircuitData};
pub use computation::{Bits, Bounds, Configs, Inputs, ShareDecryptionOutput};
