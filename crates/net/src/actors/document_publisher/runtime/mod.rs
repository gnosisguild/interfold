// SPDX-License-Identifier: LGPL-3.0-only

//! Bounded network effects dispatched by the actor shell.

use super::*;

mod io;

pub use io::{handle_document_published_notification, handle_publish_document_requested};
