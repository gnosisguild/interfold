// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::prelude::*;
use anyhow::{ensure, Result};
use e3_data::{AutoPersist, Persistable, Repository};
use e3_events::{
    prelude::*, BusHandle, CommitteeFinalizeRequested, CommitteeFinalized, CommitteeRequested,
    E3Failed, E3RequestComplete, E3Stage, E3StageChanged, EType, EffectsEnabled, EventType,
    InterfoldEvent, InterfoldEventData, Shutdown, TicketGenerated, TypedEvent,
};
use e3_events::{E3id, EventContext, Sequenced};
use e3_evm::helpers::{ConcreteReadProvider, EthProvider};
use e3_utils::{NotifySync, MAILBOX_LIMIT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{error, info};

#[path = "handlers.rs"]
mod handlers;

const FINALIZATION_BUFFER_SECONDS: u64 = 1;
const FINALIZE_INTERVAL_SECONDS: u64 = 5;
const FINALIZATION_RPC_RETRY_SECONDS: u64 = 30;
pub const COMMITTEE_FINALIZER_RECOVERY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveredCommitteeRequest {
    pub request: CommitteeRequested,
    pub context: EventContext<Sequenced>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitteeFinalizerRecoveryState {
    pub schema_version: u32,
    pub pending_requests: HashMap<E3id, RecoveredCommitteeRequest>,
    pub tickets: HashMap<E3id, TicketGenerated>,
}

impl Default for CommitteeFinalizerRecoveryState {
    fn default() -> Self {
        Self {
            schema_version: COMMITTEE_FINALIZER_RECOVERY_SCHEMA_VERSION,
            pending_requests: HashMap::new(),
            tickets: HashMap::new(),
        }
    }
}

impl CommitteeFinalizerRecoveryState {
    pub fn remove(&mut self, e3_id: &E3id) {
        self.pending_requests.remove(e3_id);
        self.tickets.remove(e3_id);
    }
}

/// CommitteeFinalizer is an actor that listens to CommitteeRequested events and dispatches
/// CommitteeFinalizeRequested events after the submission deadline has passed.
pub struct CommitteeFinalizer {
    bus: BusHandle,
    pending_committees: HashMap<E3id, SpawnHandle>,
    recovery: Persistable<CommitteeFinalizerRecoveryState>,
    chain_providers: HashMap<u64, EthProvider<ConcreteReadProvider>>,
    effects_enabled: bool,
}

impl CommitteeFinalizer {
    fn from_recovery(
        bus: &BusHandle,
        recovery: Persistable<CommitteeFinalizerRecoveryState>,
        chain_providers: HashMap<u64, EthProvider<ConcreteReadProvider>>,
    ) -> Self {
        Self {
            bus: bus.clone(),
            pending_committees: HashMap::new(),
            recovery,
            chain_providers,
            effects_enabled: false,
        }
    }

    pub async fn attach_with_recovery(
        bus: &BusHandle,
        repository: Repository<CommitteeFinalizerRecoveryState>,
        chain_providers: HashMap<u64, EthProvider<ConcreteReadProvider>>,
    ) -> Result<Addr<Self>> {
        let recovery = repository
            .load_or_default(CommitteeFinalizerRecoveryState::default())
            .await?;
        ensure!(
            recovery.try_get()?.schema_version == COMMITTEE_FINALIZER_RECOVERY_SCHEMA_VERSION,
            "unsupported committee-finalizer recovery schema"
        );
        let addr = CommitteeFinalizer::from_recovery(bus, recovery, chain_providers).start();

        // Subscribe to state-building / cleanup events immediately
        bus.subscribe_all(
            &[
                EventType::Shutdown,
                EventType::E3Failed,
                EventType::E3StageChanged,
                EventType::E3RequestComplete,
                EventType::TicketGenerated,
                EventType::CommitteeRequested,
                EventType::CommitteeFinalized,
                EventType::EffectsEnabled,
            ],
            addr.clone().recipient(),
        );

        Ok(addr)
    }

    fn schedule_committee(
        &mut self,
        request: RecoveredCommitteeRequest,
        party_index: u64,
        ctx: &mut Context<Self>,
    ) {
        let e3_id = request.request.e3_id.clone();
        if self.pending_committees.contains_key(&e3_id) {
            return;
        }

        let committee_deadline = request.request.committee_deadline;
        let request_e3_id = request.request.e3_id.clone();
        let ec = request.context.clone();
        let pending_key = e3_id.clone();
        let e3_id_for_async = e3_id.clone();
        let provider = self.chain_providers.get(&e3_id.chain_id()).cloned();

        let fut = async move {
            let timestamp = match provider {
                Some(provider) => {
                    e3_evm::helpers::get_current_timestamp_from_provider(provider).await
                }
                None => Err(anyhow::anyhow!(
                    "No RPC provider configured for chain {}",
                    e3_id_for_async.chain_id()
                )),
            };
            match timestamp {
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

        let handle = ctx.spawn(
            fut.into_actor(self)
                .then(move |current_timestamp, act, ctx| {
                    if let Some(current_timestamp) = current_timestamp {
                        let seconds_until_deadline = committee_deadline
                            .saturating_sub(current_timestamp)
                            .saturating_add(FINALIZATION_BUFFER_SECONDS)
                            .saturating_add(
                                party_index.saturating_mul(FINALIZE_INTERVAL_SECONDS),
                            );

                        info!(
                            e3_id = %e3_id,
                            party_index,
                            committee_deadline,
                            current_timestamp,
                            seconds_to_wait = seconds_until_deadline,
                            "Scheduling committee finalization"
                        );

                        let e3_id_clone = e3_id.clone();
                        let ec_clone = ec.clone();

                        let handle = ctx.run_later(
                            Duration::from_secs(seconds_until_deadline),
                            move |act, _ctx| {
                                info!(e3_id = %e3_id_clone, party_index, "Dispatching CommitteeFinalizeRequested event");
                                act.pending_committees.remove(&e3_id_clone);
                                if let Err(error) = act.bus.publish(
                                    CommitteeFinalizeRequested {
                                        e3_id: request_e3_id.clone(),
                                    },
                                    ec_clone.clone(),
                                ) {
                                    act.bus.with_ec(&ec_clone).err(EType::Sortition, error);
                                    act.schedule_retry(e3_id_clone.clone(), _ctx);
                                }
                            },
                        );

                        act.pending_committees.insert(e3_id.clone(), handle);
                    } else {
                        act.schedule_retry(e3_id.clone(), ctx);
                    }

                    async {}.into_actor(act)
                }),
        );
        self.pending_committees.insert(pending_key, handle);
    }

    fn schedule_retry(&mut self, e3_id: E3id, ctx: &mut Context<Self>) {
        let retry_e3_id = e3_id.clone();
        let handle = ctx.run_later(
            Duration::from_secs(FINALIZATION_RPC_RETRY_SECONDS),
            move |actor, ctx| {
                actor.pending_committees.remove(&retry_e3_id);
                actor.schedule_if_ready(&retry_e3_id, ctx);
            },
        );
        self.pending_committees.insert(e3_id, handle);
    }

    fn schedule_if_ready(&mut self, e3_id: &E3id, ctx: &mut Context<Self>) {
        if !self.effects_enabled {
            return;
        }

        let Some(recovery) = self.recovery.get() else {
            return;
        };
        let Some(request) = recovery.pending_requests.get(e3_id) else {
            return;
        };
        let Some(party_index) = recovery
            .tickets
            .get(e3_id)
            .and_then(|ticket| ticket.party_index)
        else {
            return;
        };

        self.schedule_committee(request.clone(), party_index, ctx);
    }
}

impl Actor for CommitteeFinalizer {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
    }
}
