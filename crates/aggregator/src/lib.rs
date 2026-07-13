// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Public-key and threshold-plaintext aggregation.
//!
//! The crate is organised into three layers:
//! - [`actors`] — thin actix actors that own persistence and the event bus and
//!   route messages between the protocol and workflow services.
//! - [`workflow`] — persisted aggregation state machines and deterministic transitions.
//! - [`domain`] — pure protocol values, invariants, and calculations.

mod actors;
mod domain;
pub mod ext;
mod repo;
mod workflow;

pub use actors::*;
pub use domain::committee_hash;
pub use repo::*;
