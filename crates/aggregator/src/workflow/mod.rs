// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Persisted aggregation workflows and deterministic transition services.
//!
//! These modules own protocol progress but perform no Actix, persistence, or bus I/O. The actor
//! shells commit their state transitions and execute the resulting runtime work.

pub mod publickey_aggregation;
pub mod threshold_plaintext_aggregation;
