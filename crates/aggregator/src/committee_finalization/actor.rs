// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::prelude::*;
use e3_events::{
    prelude::*, trap, BusHandle, CommitteeFinalizeRequested, CommitteeRequested, E3Failed,
    E3RequestComplete, E3Stage, E3StageChanged, EType, EffectsEnabled, EventType, InterfoldEvent,
    InterfoldEventData, Shutdown, TicketGenerated, TypedEvent,
};
use e3_events::{E3id, EventContext, Sequenced};
use e3_utils::{NotifySync, MAILBOX_LIMIT};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{error, info};

#[path = "handlers.rs"]
mod handlers;

const FINALIZATION_BUFFER_SECONDS: u64 = 1;
const FINALIZE_INTERVAL_SECONDS: u64 = 5;

#[derive(Clone)]
struct PendingCommitteeRequest {
    e3_id: E3id,
    committee_deadline: u64,
    ec: EventContext<Sequenced>,
}

/// CommitteeFinalizer is an actor that listens to CommitteeRequested events and dispatches
/// CommitteeFinalizeRequested events after the submission deadline has passed.
pub struct CommitteeFinalizer {
    bus: BusHandle,
    pending_committees: HashMap<String, SpawnHandle>,
    pending_requests: HashMap<String, PendingCommitteeRequest>,
    party_indexes: HashMap<String, u64>,
    effects_enabled: bool,
}

impl CommitteeFinalizer {
    pub fn new(bus: &BusHandle) -> Self {
        Self {
            bus: bus.clone(),
            pending_committees: HashMap::new(),
            pending_requests: HashMap::new(),
            party_indexes: HashMap::new(),
            effects_enabled: false,
        }
    }

    pub fn attach(bus: &BusHandle) -> Addr<Self> {
        let addr = CommitteeFinalizer::new(bus).start();

        // Subscribe to state-building / cleanup events immediately
        bus.subscribe_all(
            &[
                EventType::Shutdown,
                EventType::E3Failed,
                EventType::E3StageChanged,
                EventType::E3RequestComplete,
                EventType::TicketGenerated,
                EventType::CommitteeRequested,
                EventType::EffectsEnabled,
            ],
            addr.clone().recipient(),
        );

        addr
    }

    fn schedule_committee(
        &mut self,
        e3_id: String,
        request: PendingCommitteeRequest,
        party_index: u64,
        ctx: &mut Context<Self>,
    ) {
        if self.pending_committees.contains_key(&e3_id) {
            return;
        }

        let committee_deadline = request.committee_deadline;
        let request_e3_id = request.e3_id.clone();
        let ec = request.ec.clone();
        let e3_id_for_async = e3_id.clone();

        let fut = async move {
            match e3_evm::helpers::get_current_timestamp().await {
                Ok(timestamp) => Some(timestamp),
                Err(e) => {
                    error!(
                        e3_id = %e3_id_for_async,
                        error = %e,
                        "Failed to get current timestamp from RPC"
                    );
                    None
                }
            }
        };

        ctx.spawn(
            fut.into_actor(self)
                .then(move |current_timestamp, act, ctx| {
                    if let Some(current_timestamp) = current_timestamp {
                        let seconds_until_deadline = committee_deadline.saturating_sub(current_timestamp) + FINALIZATION_BUFFER_SECONDS
                            + (party_index * FINALIZE_INTERVAL_SECONDS);

                        info!(
                            e3_id = %e3_id,
                            party_index,
                            committee_deadline,
                            current_timestamp,
                            seconds_to_wait = seconds_until_deadline,
                            "Scheduling committee finalization"
                        );

                        let bus = act.bus.clone();
                        let e3_id_clone = e3_id.clone();
                        let ec_clone = ec.clone();

                        let handle = ctx.run_later(
                            Duration::from_secs(seconds_until_deadline),
                            move |act, _ctx| {
                                info!(e3_id = %e3_id_clone, party_index, "Dispatching CommitteeFinalizeRequested event");

                                trap(EType::Sortition, &act.bus.with_ec(&ec_clone), || {
                                    bus.publish(
                                        CommitteeFinalizeRequested {
                                            e3_id: request_e3_id.clone(),
                                        },
                                        ec_clone.clone(),
                                    )?;
                                    Ok(())
                                });

                                act.pending_committees.remove(&e3_id_clone);
                            },
                        );

                        act.pending_committees.insert(e3_id.clone(), handle);
                    }

                    async {}.into_actor(act)
                }),
        );
    }

    fn schedule_if_ready(&mut self, e3_id: &str, ctx: &mut Context<Self>) {
        if !self.effects_enabled {
            return;
        }

        let Some(request) = self.pending_requests.get(e3_id).cloned() else {
            return;
        };
        let Some(party_index) = self.party_indexes.get(e3_id).copied() else {
            return;
        };

        self.schedule_committee(e3_id.to_owned(), request, party_index, ctx);
    }
}

impl Actor for CommitteeFinalizer {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
    }
}
