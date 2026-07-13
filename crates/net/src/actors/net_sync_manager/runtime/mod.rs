// SPDX-License-Identifier: LGPL-3.0-only

//! Bounded network effects and lifecycle coordination.

use super::*;

mod admission;
pub(super) mod historical_sync;
mod readiness;
mod rebroadcast;
