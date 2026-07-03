// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE

use crate::traits::Provable;
use e3_events::CircuitName;
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::circuits::Individual_key::pk::circuit::{PkCircuit, PkCircuitData};
use e3_zk_helpers::circuits::Individual_key::pk::computation::Inputs;

impl Provable for PkCircuit {
    type Params = BfvPreset;
    type Input = PkCircuitData;
    type Inputs = Inputs;

    fn circuit(&self) -> CircuitName {
        CircuitName::PkBfv
    }
}
