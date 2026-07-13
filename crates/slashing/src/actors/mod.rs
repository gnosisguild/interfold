// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Thin actix shells that translate [`InterfoldEvent`]s into workflow inputs
//! and perform the effects returned by deterministic transitions.
//!
//! [`InterfoldEvent`]: e3_events::InterfoldEvent

pub mod accusation_manager;
pub mod commitment_consistency_checker;
