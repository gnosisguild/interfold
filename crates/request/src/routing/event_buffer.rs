// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use e3_events::{E3id, InterfoldEvent};
use std::collections::HashMap;

/// Buffers events for downstream instances to handle out-of-order event delivery.
/// Events are scoped by protocol request and recipient until the recipient is ready.
#[derive(Default)]
pub struct EventBuffer {
    buffer: HashMap<(E3id, String), Vec<InterfoldEvent>>,
}

impl EventBuffer {
    pub fn add(&mut self, e3_id: &E3id, recipient: &str, event: InterfoldEvent) {
        self.buffer
            .entry((e3_id.clone(), recipient.to_owned()))
            .or_default()
            .push(event)
    }

    pub fn take(&mut self, e3_id: &E3id, recipient: &str) -> Vec<InterfoldEvent> {
        self.buffer
            .remove(&(e3_id.clone(), recipient.to_owned()))
            .unwrap_or_default()
    }

    /// Discard data for a terminal request, including recipients that were never constructed on
    /// this node role.
    pub fn remove_e3(&mut self, e3_id: &E3id) {
        self.buffer
            .retain(|(buffered_id, _), _| buffered_id != e3_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use e3_events::{E3id, InterfoldEvent, Sequenced};

    fn event(label: &str) -> InterfoldEvent {
        InterfoldEvent::<Sequenced>::test_event(label)
            .e3_id(E3id::new("1", 1))
            .seq(1)
            .build()
    }

    #[test]
    fn take_returns_empty_for_unknown_key() {
        let mut buffer = EventBuffer::default();
        assert!(buffer.take(&E3id::new("1", 1), "missing").is_empty());
    }

    #[test]
    fn add_then_take_drains_buffer() {
        let mut buffer = EventBuffer::default();
        let e3_id = E3id::new("1", 1);
        buffer.add(&e3_id, "k", event("a"));
        buffer.add(&e3_id, "k", event("b"));

        let drained = buffer.take(&e3_id, "k");
        assert_eq!(drained.len(), 2);
        // A second take should yield nothing since the buffer was drained.
        assert!(buffer.take(&e3_id, "k").is_empty());
    }

    #[test]
    fn keys_are_isolated() {
        let mut buffer = EventBuffer::default();
        let e3_id = E3id::new("1", 1);
        buffer.add(&e3_id, "a", event("x"));
        buffer.add(&e3_id, "b", event("y"));

        assert_eq!(buffer.take(&e3_id, "a").len(), 1);
        assert_eq!(buffer.take(&e3_id, "b").len(), 1);
    }

    #[test]
    fn terminal_cleanup_removes_only_the_completed_e3() {
        let mut buffer = EventBuffer::default();
        let completed = E3id::new("1", 1);
        let active = E3id::new("2", 1);
        buffer.add(&completed, "missing-a", event("old-a"));
        buffer.add(&completed, "missing-b", event("old-b"));
        buffer.add(&active, "missing-a", event("active"));

        buffer.remove_e3(&completed);

        assert!(buffer.take(&completed, "missing-a").is_empty());
        assert!(buffer.take(&completed, "missing-b").is_empty());
        assert_eq!(buffer.take(&active, "missing-a").len(), 1);
    }
}
