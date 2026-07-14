// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Compatibility view of pure sortition modules stored by capability.

#[path = "sortition/selection_backend.rs"]
pub mod backends;
#[path = "sortition/finalized_committee_retention.rs"]
mod finalized_committee_retention;
#[path = "sortition/node_registry.rs"]
pub mod node_registry;
#[path = "sortition/ticket.rs"]
pub mod ticket;
#[path = "sortition/ticket_selection.rs"]
pub mod ticket_sortition;

pub use backends::*;
pub use finalized_committee_retention::*;
pub use node_registry::*;
pub use ticket::*;
pub use ticket_sortition::*;
