// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE

use crate::traits::Provable;
use e3_events::CircuitName;
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::circuits::Threshold_key::share_decryption::circuit::{
    ShareDecryptionCircuit, ShareDecryptionCircuitData,
};
use e3_zk_helpers::circuits::Threshold_key::share_decryption::computation::Inputs;

impl Provable for ShareDecryptionCircuit {
    type Params = BfvPreset;
    type Input = ShareDecryptionCircuitData;
    type Inputs = Inputs;

    fn circuit(&self) -> CircuitName {
        CircuitName::ThresholdShareDecryption
    }
}
