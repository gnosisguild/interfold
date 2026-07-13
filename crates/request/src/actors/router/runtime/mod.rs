// SPDX-License-Identifier: LGPL-3.0-only

//! Router construction and durable snapshot integration.

use super::*;

mod builder;
mod snapshot;

pub use builder::E3RouterBuilder;
pub use snapshot::E3RouterSnapshot;
