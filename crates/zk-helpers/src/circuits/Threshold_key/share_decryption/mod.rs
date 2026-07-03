// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Threshold share decryption circuit.
//!
//! Proves correct computation of a BFV decryption share: the prover shows that their
//! share `d` satisfies the lifted relation
//! `d = c_0 + c_1 * s + e + r_2 * (X^N + 1) + r_1 * q_i` with committed `s` and `e`,
//! and produces Prover.toml and configs.nr for the Noir prover. See [`ShareDecryptionCircuit`]
//! and [`ShareDecryptionCircuitInput`].

pub mod circuit;
pub mod codegen;
pub mod computation;
pub mod sample;
pub use circuit::*;
pub use codegen::*;
pub use computation::*;
