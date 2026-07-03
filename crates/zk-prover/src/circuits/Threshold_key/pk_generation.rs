// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE

use crate::traits::Provable;
use e3_events::CircuitName;
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::circuits::Threshold_key::pk_generation::circuit::{
    PkGenerationCircuit, PkGenerationCircuitData,
};
use e3_zk_helpers::circuits::Threshold_key::pk_generation::computation::Inputs;

impl Provable for PkGenerationCircuit {
    type Params = BfvPreset;
    type Input = PkGenerationCircuitData;
    type Inputs = Inputs;

    fn circuit(&self) -> CircuitName {
        CircuitName::PkGeneration
    }
}
