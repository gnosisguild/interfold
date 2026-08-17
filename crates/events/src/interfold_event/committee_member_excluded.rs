// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::{E3id, ProofType};
use actix::Message;
use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

/// Records an E3-scoped exclusion after the committee confirms a proof fault and the matching
/// on-chain slash policy is disabled.
///
/// This event does not mean that the node was slashed, banned, or removed from the on-chain
/// committee. It lets the current E3 stop waiting for known-bad work without changing the
/// canonical committee roster used by proofs.
#[derive(Message, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct CommitteeMemberExcluded {
    /// The E3 computation that excludes the member.
    pub e3_id: E3id,
    /// Address of the excluded committee member.
    pub node: Address,
    /// The proof type confirmed as faulty by accusation quorum.
    pub proof_type: ProofType,
    /// Party ID in the finalized committee.
    ///
    /// This is `None` when the slashing writer creates the event. Sortition resolves the stable
    /// party ID and republishes the event with `Some(id)` for downstream actors.
    pub party_id: Option<u64>,
}

impl Display for CommitteeMemberExcluded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CommitteeMemberExcluded {{ e3_id: {}, node: {}, proof_type: {}, party_id: {:?} }}",
            self.e3_id, self.node, self.proof_type, self.party_id
        )
    }
}
