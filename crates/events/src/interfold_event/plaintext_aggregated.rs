// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::{E3id, Proof};
use actix::Message;
use derivative::Derivative;
use e3_utils::ArcBytes;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

#[derive(Derivative, Message, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derivative(Debug)]
#[rtype(result = "()")]
/// Local durable intent to publish the aggregated plaintext on-chain.
///
/// This event is not a canonical completion fact and is not forwarded to peers. The request
/// completes only after the node observes the confirmed `E3StageChanged(Complete)` chain event.
pub struct PlaintextAggregated {
    pub e3_id: E3id,
    pub decrypted_output: Vec<ArcBytes>,
    /// Final DecryptionAggregator (EVM) proof(s): one per ciphertext index. Test/CI nodes that
    /// skip recursive aggregation carry the existing C7 proofs here as mock-verifier
    /// placeholders. On-chain publication currently only supports a single-output plaintext, in
    /// which case the first proof is forwarded to `publishPlaintextOutput`.
    pub decryption_aggregator_proofs: Vec<Proof>,
}

impl Display for PlaintextAggregated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
