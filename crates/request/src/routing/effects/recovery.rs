// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::{PostForward, RequestRouter, RoutingDecision};
use e3_events::{EventContextAccessors, EventContextSeq, InterfoldEvent, RequestRouterCheckpoint};

/// Apply one durable event to the request-router recovery projection.
///
/// This projection changes only router admission state. It does not start actors or run effects.
pub fn project_request_router_event(
    checkpoint: &mut RequestRouterCheckpoint,
    event: &InterfoldEvent,
) {
    let has_context = event
        .get_e3_id()
        .is_some_and(|e3_id| checkpoint.contexts.contains(&e3_id));

    match RequestRouter::route_with_context(event, &checkpoint.completed, has_context) {
        RoutingDecision::Process {
            e3_id,
            post_forward: PostForward::Teardown,
        } => {
            checkpoint.contexts.retain(|context| context != &e3_id);
            checkpoint.completed.insert(e3_id);
        }
        RoutingDecision::Process { e3_id, .. } => {
            if !checkpoint.contexts.contains(&e3_id) {
                checkpoint.contexts.push(e3_id);
            }
        }
        RoutingDecision::Broadcast
        | RoutingDecision::Ignore
        | RoutingDecision::AlreadyCompleted(_)
        | RoutingDecision::UnadmittedNetworkEvent(_) => {}
    }

    let sequence = event.seq();
    checkpoint
        .replay_cursors
        .entry(event.aggregate_id())
        .and_modify(|cursor| *cursor = (*cursor).max(sequence))
        .or_insert(sequence);
}

#[cfg(test)]
mod tests {
    use super::*;
    use e3_events::Unsequenced;

    fn replay_event(sequence: u64) -> InterfoldEvent {
        InterfoldEvent::<Unsequenced>::test_event("router recovery")
            .id(1)
            .aggregate_id(7)
            .seq(sequence)
            .build()
    }

    #[test]
    fn replay_cursor_keeps_highest_sequence_seen() {
        let aggregate_id = e3_events::AggregateId::new(7);
        let mut checkpoint = RequestRouterCheckpoint::default();

        project_request_router_event(&mut checkpoint, &replay_event(64));
        project_request_router_event(&mut checkpoint, &replay_event(59));

        assert_eq!(checkpoint.replay_cursors.get(&aggregate_id), Some(&64));
    }
}
