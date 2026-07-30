// SPDX-License-Identifier: LGPL-3.0-only

//! Router construction and durable snapshot integration.

use super::*;

#[path = "build_context.rs"]
mod builder;
mod snapshot;

pub use builder::E3RouterBuilder;
pub use snapshot::{load_dkg_fold_attestation_contexts, E3RouterSnapshot};
