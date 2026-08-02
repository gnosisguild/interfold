// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::{DkgFoldAttestationContext, E3id};
use actix::Message;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

pub const DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION: u8 = 1;

/// Request-time signing domain for one E3.
#[derive(Message, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct DkgFoldAttestationContextEstablished {
    pub schema_version: u8,
    pub e3_id: E3id,
    pub context: DkgFoldAttestationContext,
}

impl Display for DkgFoldAttestationContextEstablished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "e3_id: {}, registry: {}, verifying_contract: {}",
            self.e3_id, self.context.registry, self.context.verifying_contract
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommitteeFinalized, InterfoldEventData};
    use alloy::primitives::Address;

    #[test]
    fn adding_the_context_event_keeps_legacy_committee_events_readable() {
        const LEGACY_FIXTURE: &[u8] = &[
            23, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 55, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
            0, 4, 0, 0, 0, 0, 0, 0, 0, 48, 120, 48, 49, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
            0, 0, 57, 1, 0, 0, 0, 0, 0, 0, 0,
        ];

        let value: InterfoldEventData =
            bincode::deserialize(LEGACY_FIXTURE).expect("read legacy fixture");
        assert_eq!(
            value,
            InterfoldEventData::CommitteeFinalized(CommitteeFinalized {
                e3_id: crate::E3id::new("7", 1),
                committee: vec!["0x01".to_owned()],
                scores: vec!["9".to_owned()],
                chain_id: 1,
            })
        );
        assert_eq!(
            bincode::serialize(&value).expect("write legacy fixture"),
            LEGACY_FIXTURE
        );
    }

    #[test]
    fn context_event_carries_an_explicit_schema_version() {
        let value = DkgFoldAttestationContextEstablished {
            schema_version: DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
            e3_id: crate::E3id::new("8", 1),
            context: DkgFoldAttestationContext {
                registry: Address::repeat_byte(0x11),
                verifying_contract: Address::repeat_byte(0x22),
            },
        };

        let bytes = bincode::serialize(&value).expect("serialize context");
        let decoded: DkgFoldAttestationContextEstablished =
            bincode::deserialize(&bytes).expect("deserialize context");
        assert_eq!(decoded, value);
    }
}
