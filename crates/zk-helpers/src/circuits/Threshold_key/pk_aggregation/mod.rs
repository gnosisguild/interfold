// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Public key aggregation circuit.
//!
//! This circuit proves public key aggregation with a threshold BFV public key (pk0, pk1) and produces
//! Prover.toml and configs.nr for the Noir prover. See [`PkAggregationCircuit`] and
//! [`PkAggregationCircuitInput`].

pub mod circuit;
pub mod codegen;
pub mod computation;
pub mod sample;
pub use circuit::*;
pub use codegen::*;
pub use computation::*;
