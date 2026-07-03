// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE

use crate::traits::Provable;
use e3_events::CircuitName;
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::circuits::Threshold_key::pk_aggregation::circuit::{
    PkAggregationCircuit, PkAggregationCircuitData,
};
use e3_zk_helpers::circuits::Threshold_key::pk_aggregation::computation::Inputs;

impl Provable for PkAggregationCircuit {
    type Params = BfvPreset;
    type Input = PkAggregationCircuitData;
    type Inputs = Inputs;

    fn circuit(&self) -> CircuitName {
        CircuitName::PkAggregation
    }
}
