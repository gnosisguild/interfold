// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE

use crate::traits::Provable;
use e3_events::CircuitName;
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::circuits::Threshold_key::decrypted_shares_aggregation::circuit::{
    DecryptedSharesAggregationCircuit, DecryptedSharesAggregationCircuitData,
};
use e3_zk_helpers::circuits::Threshold_key::decrypted_shares_aggregation::computation::Inputs;

impl Provable for DecryptedSharesAggregationCircuit {
    type Params = BfvPreset;
    type Input = DecryptedSharesAggregationCircuitData;
    type Inputs = Inputs;

    fn circuit(&self) -> CircuitName {
        CircuitName::DecryptedSharesAggregation
    }

    fn valid_circuits(&self) -> Vec<CircuitName> {
        vec![CircuitName::DecryptedSharesAggregation]
    }
}
