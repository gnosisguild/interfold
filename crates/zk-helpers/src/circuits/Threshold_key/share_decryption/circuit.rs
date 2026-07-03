// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Circuit type and input for threshold share decryption.

use crate::computation::DkgInputType;
use crate::registry::Circuit;
use e3_fhe_params::ParameterType;
use e3_polynomial::CrtPolynomial;
use fhe::bfv::{Ciphertext, PublicKey};

/// Threshold share decryption circuit (PVSS #6).
///
/// Verifies correct computation of a party's decryption share with respect to
/// committed aggregated secret and smudging-error shares.
#[derive(Debug)]
pub struct ShareDecryptionCircuit;

/// Input to the share decryption circuit: ciphertext, public key, and the party's
/// aggregated secret share (s), smudging error (e), and computed decryption share (d_share).
pub struct ShareDecryptionCircuitData {
    pub ciphertext: Ciphertext,
    pub public_key: PublicKey,
    pub s: CrtPolynomial,
    pub e: CrtPolynomial,
    pub d_share: CrtPolynomial,
}

impl Circuit for ShareDecryptionCircuit {
    const NAME: &'static str = "threshold-share-decryption";
    const PREFIX: &'static str = "THRESHOLD_SHARE_DECRYPTION";
    const SUPPORTED_PARAMETER: ParameterType = ParameterType::THRESHOLD;
    const DKG_INPUT_TYPE: Option<DkgInputType> = None;
}
