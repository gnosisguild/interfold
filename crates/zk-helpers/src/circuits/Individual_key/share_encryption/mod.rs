// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Share-encryption circuit: proves correct encryption of a share under the DKG public key (CIRCUIT 3a/3b).

pub mod circuit;
pub mod codegen;
pub mod computation;
pub mod sample;
pub use circuit::{ShareEncryptionCircuit, ShareEncryptionCircuitData};
pub use computation::{Bits, Bounds, Configs, Inputs, ShareEncryptionOutput};
