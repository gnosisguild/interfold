// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! E3Extension that wires up the [`AccusationManager`] per-E3 when the
//! committee is finalized.
//!
//! Listens for [`CommitteeFinalized`], derives the on-chain accusation quorum
//! from the circuit threshold in [`E3Meta`], parses committee addresses, and
//! starts the actor with full context.

use std::collections::HashMap;

use crate::actors::accusation_manager::AccusationManager;
use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use anyhow::Result;
use async_trait::async_trait;
use e3_events::{BusHandle, CommitteeFinalized, Event, InterfoldEvent, InterfoldEventData};
use e3_request::{E3Context, E3ContextSnapshot, E3Extension, META_KEY};
use e3_zk_helpers::CiphernodesCommitteeSize;
use tracing::{error, info, warn};

/// Convert the compiled polynomial threshold `T` and committee size `N` into
/// the honest-party count `H` used by `SlashingManager` as its vote quorum.
///
/// `E3Meta.threshold_m` intentionally carries `T` for circuit selection, while
/// the on-chain committee request and slashing contract use `H = T + 1`. Keep
/// both values separate so accusation voting agrees with Solidity without
/// breaking ZK re-verification artifact resolution.
fn accusation_vote_quorum(threshold_t: usize, committee_n: usize) -> Result<usize> {
    Ok(
        CiphernodesCommitteeSize::from_threshold(threshold_t, committee_n)?
            .values()
            .h,
    )
}

pub struct AccusationManagerExtension {
    bus: BusHandle,
    signer: PrivateKeySigner,
    /// On-chain `SlashingManager` address (EIP-712 `verifyingContract` for vote sigs).
    slashing_manager: Address,
    /// Per-chain off-chain freshness window (seconds), read from
    /// `CiphernodeRegistry.accusationVoteValidity()` at process startup.
    /// Looked up by `e3_id.chain_id()` when each per-E3 actor starts;
    /// governance changes require a node restart to take effect (same lifecycle
    /// contract as `slashing_manager`).
    vote_validity_secs_by_chain: HashMap<u64, u64>,
    /// Clock-skew allowance for peer accusation deadlines.
    accusation_deadline_skew_secs: u64,
}

impl AccusationManagerExtension {
    pub fn create(
        bus: &BusHandle,
        signer: PrivateKeySigner,
        slashing_manager: Address,
        vote_validity_secs_by_chain: HashMap<u64, u64>,
        accusation_deadline_skew_secs: u64,
    ) -> Box<Self> {
        Box::new(Self {
            bus: bus.clone(),
            signer: signer.clone(),
            slashing_manager,
            vote_validity_secs_by_chain,
            accusation_deadline_skew_secs,
        })
    }

    fn vote_validity_secs_for(&self, chain_id: u64) -> u64 {
        match self.vote_validity_secs_by_chain.get(&chain_id) {
            Some(&secs) => secs,
            None => {
                warn!(
                    chain_id,
                    "no accusationVoteValidity configured for chain; accusation votes will not be stamped"
                );
                0
            }
        }
    }
}

#[async_trait]
impl E3Extension for AccusationManagerExtension {
    fn on_event(&self, ctx: &mut E3Context, evt: &InterfoldEvent) {
        let InterfoldEventData::CommitteeFinalized(data) = evt.get_data() else {
            return;
        };

        // Don't start twice
        if ctx.get_event_recipient("accusation_manager").is_some() {
            return;
        }

        let CommitteeFinalized {
            e3_id, committee, ..
        } = data.clone();

        // Parse committee addresses — all must be valid or we cannot start
        let mut committee_addresses: Vec<Address> = Vec::with_capacity(committee.len());
        for s in committee.iter() {
            match s.parse::<Address>() {
                Ok(addr) => committee_addresses.push(addr),
                Err(e) => {
                    error!(
                        "Failed to parse committee address {} — cannot start AccusationManager: {}",
                        s, e
                    );
                    return;
                }
            }
        }

        if committee_addresses.is_empty() {
            error!("No committee addresses — cannot start AccusationManager");
            return;
        }

        // `E3Meta` stores the compiled circuit threshold T. Solidity requires
        // H votes, so derive and pass both values explicitly.
        let Some(meta) = ctx.get_dependency(META_KEY) else {
            error!("E3Meta not available — cannot start AccusationManager");
            return;
        };
        let circuit_threshold_t = meta.threshold_m;
        let vote_quorum_h = match accusation_vote_quorum(meta.threshold_m, meta.threshold_n) {
            Ok(quorum) => quorum,
            Err(err) => {
                error!(
                    %e3_id,
                    threshold_t = meta.threshold_m,
                    committee_n = meta.threshold_n,
                    error = %err,
                    "Unknown committee size — cannot start AccusationManager"
                );
                return;
            }
        };

        info!(
            "Starting AccusationManager for E3 {} with {} committee members, circuit threshold T={}, vote quorum H={}",
            e3_id,
            committee_addresses.len(),
            circuit_threshold_t,
            vote_quorum_h
        );

        let vote_validity_secs = self.vote_validity_secs_for(e3_id.chain_id());

        let addr = AccusationManager::setup_with_quorum(
            &self.bus,
            e3_id,
            self.signer.clone(),
            self.slashing_manager,
            committee_addresses,
            circuit_threshold_t,
            vote_quorum_h,
            vote_validity_secs,
            self.accusation_deadline_skew_secs,
            meta.params_preset,
        );

        ctx.set_event_recipient("accusation_manager", Some(addr.into()));
    }

    /// Re-hydrates the `AccusationManager` after a node restart.
    ///
    /// Intentionally a no-op — `AccusationManager` is **ephemeral by design**:
    ///
    /// - Each instance is scoped to one E3 (created by [`AccusationManagerExtension::handle`]
    ///   when `CommitteeFinalized` is received) and holds only transient in-memory state
    ///   (pending accusations, buffered votes, verification caches).
    /// - On restart, all in-flight accusations are lost. This is an accepted trade-off:
    ///   every pending accusation has a finite vote timeout (default 5 min). If the node
    ///   restarts, the accusation would have timed out anyway. Other committee members
    ///   running their own independent `AccusationManager` instances will continue the
    ///   protocol unaffected.
    /// - A malicious node cannot exploit restart-induced state loss to prevent slashing:
    ///   restarting only loses *this node's* pending state — all other honest nodes still
    ///   independently verify, vote, and reach quorum without this node's participation
    ///   (as long as enough honest nodes remain to meet the on-chain quorum H).
    async fn hydrate(&self, _ctx: &mut E3Context, _snapshot: &E3ContextSnapshot) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accusation_quorum_matches_canonical_on_chain_committee_thresholds() {
        for (threshold_t, committee_n, expected_h) in [(1, 3, 2), (4, 9, 5), (9, 19, 10)] {
            assert_eq!(
                accusation_vote_quorum(threshold_t, committee_n).unwrap(),
                expected_h
            );
        }
    }

    #[test]
    fn accusation_quorum_rejects_unknown_committee_parameters() {
        assert!(accusation_vote_quorum(2, 3).is_err());
    }
}
