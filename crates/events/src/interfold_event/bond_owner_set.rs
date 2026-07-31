// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::Message;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

#[derive(Message, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct BondOwnerSet {
    pub operator: String,
    pub bond_owner: String,
    pub chain_id: u64,
}

impl Display for BondOwnerSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BondOwnerSet {{ operator: {}, bond_owner: {}, chain_id: {} }}",
            self.operator, self.bond_owner, self.chain_id
        )
    }
}
