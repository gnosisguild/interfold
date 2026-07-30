// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Computation types for the share-computation circuit: constants, bounds, bit widths, and input.
//!
//! [`Configs`], [`Bounds`], [`Bits`], and [`Inputs`] are produced from BFV parameters
//! and (for input) secret plus shares. Input values are normalized to [0, q_j) per modulus
//! and then to the ZKP field modulus so the Noir circuit's range check and parity check succeed.

use crate::bigint_3d_to_json_values;
use crate::circuits::commitments::{
    compute_sc_esm_secret_root_commitment, compute_sc_sk_secret_root_commitment,
};
use crate::computation::DkgInputType;
use crate::dkg::share_computation::ShareComputationCircuit;
use crate::dkg::share_computation::ShareComputationCircuitData;
use crate::CircuitsErrors;
use crate::{calculate_bit_width, crt_polynomial_to_toml_json, poly_coefficients_to_toml_json};
use crate::{CircuitComputation, Computation};
use e3_fhe_params::build_pair_for_preset;
use e3_fhe_params::BfvPreset;
use e3_polynomial::{reduce, CrtPolynomial, Polynomial};
use fhe::bfv::SecretKey;
use fhe::trbfv::{SmudgingBoundCalculator, SmudgingBoundCalculatorConfig};
use num_bigint::{BigInt, BigUint};
use serde::{Deserialize, Serialize};

/// Output of [`CircuitComputation::compute`] for [`ShareComputationCircuit`]: bounds, bit widths, and input.
#[derive(Debug)]
pub struct ShareComputationOutput {
    pub bounds: Bounds,
    pub bits: Bits,
    pub inputs: Inputs,
}

/// Implementation of [`CircuitComputation`] for [`ShareComputationCircuit`].
impl CircuitComputation for ShareComputationCircuit {
    type Preset = BfvPreset;
    type Data = ShareComputationCircuitData;
    type Output = ShareComputationOutput;
    type Error = CircuitsErrors;

    fn compute(preset: Self::Preset, data: &Self::Data) -> Result<Self::Output, Self::Error> {
        let bounds = Bounds::compute(preset, data)?;
        let bits = Bits::compute(preset, &bounds)?;
        let inputs = Inputs::compute(preset, data)?;

        Ok(ShareComputationOutput {
            bounds,
            bits,
            inputs,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configs {
    pub n: usize,
    pub l: usize,
    pub chunk_size: usize,
    pub n_chunks: usize,
    pub chunks_per_batch: usize,
    pub n_batches: usize,
    pub moduli: Vec<u64>,
    pub bits: Bits,
    pub bounds: Bounds,
}

/// Bit widths used by the Noir prover (e.g. for packing coefficients).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bits {
    pub bit_sk_secret: u32,
    pub bit_e_sm_secret: u32,
    pub bit_share: u32,
}

/// Coefficient bounds for public key polynomials (used to derive bit widths).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub sk_bound: BigUint,
    pub e_sm_bound: BigUint,
}

/// Input for the share-computation circuit: secret in CRT form, y (secret + shares per coeff/modulus), and commitment.
///
/// All coefficients are reduced to the ZKP field modulus for serialization. Before that,
/// secret_crt and y are normalized so that per modulus j: secret and shares are in [0, q_j),
/// ensuring the circuit's secret consistency (y[i][j][0] == e_sm_secret[j][i]), range check, and parity check pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inputs {
    /// Secret polynomial in CRT form (SK or smudging noise). Coefficients in [0, zkp_modulus) for serialization.
    pub secret_crt: CrtPolynomial,
    /// y[coeff_idx][mod_idx][0] = secret at (mod_idx, coeff_idx); y[coeff_idx][mod_idx][1 + party] = share for party. Values in [0, zkp_modulus).
    pub y: Vec<Vec<Vec<BigInt>>>,
    /// Expected secret commitment (matches C1's compute_secret_commitment).
    pub expected_secret_commitment: BigInt,
    /// Which secret type this witness is for (determines which circuit to run).
    pub dkg_input_type: DkgInputType,
}

/// Fixed-width private witness for one share-computation chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct ChunkInputs {
    pub chunk_idx: usize,
    pub secret_crt: CrtPolynomial,
    pub y_chunk: Vec<Vec<Vec<BigInt>>>,
    pub dkg_input_type: DkgInputType,
}

/// Reference chunk width used by the insecure and secure presets.
pub const SHARE_COMPUTATION_CHUNK_SIZE: usize = 512;

/// Return the number of fixed-width chunks required for a polynomial.
pub fn chunk_count(n: usize, chunk_size: usize) -> usize {
    assert!(chunk_size > 0);
    (n + chunk_size - 1) / chunk_size
}

/// Return the configured batch width for a polynomial degree.
pub fn chunks_per_batch(n: usize) -> usize {
    if n <= SHARE_COMPUTATION_CHUNK_SIZE {
        1
    } else {
        4
    }
}

/// Return the number of batches required for the configured chunk width.
pub fn batch_count(n_chunks: usize, chunks_per_batch: usize) -> usize {
    assert!(chunks_per_batch > 0);
    (n_chunks + chunks_per_batch - 1) / chunks_per_batch
}

impl Inputs {
    /// Split the full witness into fixed-width, zero-padded private chunks.
    pub fn split_into_chunks(&self, chunk_size: usize) -> Result<Vec<ChunkInputs>, CircuitsErrors> {
        if chunk_size == 0 {
            return Err(CircuitsErrors::Sample(
                "chunk size must be greater than zero".into(),
            ));
        }
        if self.y.is_empty() {
            return Err(CircuitsErrors::Sample(
                "share-computation witness has no coefficients".into(),
            ));
        }

        let n = self.y.len();
        let n_chunks = chunk_count(n, chunk_size);
        let l = self.y[0].len();
        let width = self.y[0].first().map_or(0, Vec::len);
        if l == 0 || width == 0 {
            return Err(CircuitsErrors::Sample(
                "share-computation witness has an empty row".into(),
            ));
        }
        if self.secret_crt.limbs.len() != l {
            return Err(CircuitsErrors::Sample(format!(
                "secret limb count {} does not match y modulus count {}",
                self.secret_crt.limbs.len(),
                l
            )));
        }

        let zero_row = || vec![vec![BigInt::from(0u8); width]; l];
        let mut chunks = Vec::with_capacity(n_chunks);
        for chunk_idx in 0..n_chunks {
            let start = chunk_idx * chunk_size;
            let mut y_chunk = Vec::with_capacity(chunk_size);
            for offset in 0..chunk_size {
                y_chunk.push(
                    self.y
                        .get(start + offset)
                        .cloned()
                        .unwrap_or_else(|| zero_row()),
                );
            }

            let limbs = self
                .secret_crt
                .limbs
                .iter()
                .map(|limb| {
                    let coefficients = (0..chunk_size)
                        .map(|offset| {
                            limb.coefficients()
                                .get(start + offset)
                                .cloned()
                                .unwrap_or_else(|| BigInt::from(0u8))
                        })
                        .collect();
                    Polynomial::new(coefficients)
                })
                .collect();

            chunks.push(ChunkInputs {
                chunk_idx,
                secret_crt: CrtPolynomial::new(limbs),
                y_chunk,
                dkg_input_type: self.dkg_input_type,
            });
        }

        Ok(chunks)
    }
}

impl ChunkInputs {
    pub fn to_json(&self) -> serde_json::Result<serde_json::Value> {
        let secret = match self.dkg_input_type {
            DkgInputType::SecretKey => serde_json::json!({
                "coefficients": self
                    .secret_crt
                    .limb(0)
                    .coefficients()
                    .iter()
                    .map(crate::bigint_to_json_value)
                    .collect::<Vec<_>>(),
            }),
            DkgInputType::SmudgingNoise => {
                serde_json::Value::Array(crt_polynomial_to_toml_json(&self.secret_crt))
            }
        };

        Ok(serde_json::json!({
            "chunk_idx": self.chunk_idx,
            "secret_chunk": secret,
            "y_chunk": bigint_3d_to_json_values(&self.y_chunk),
        }))
    }
}

impl Serialize for ChunkInputs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_json()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

impl Computation for Configs {
    type Preset = BfvPreset;
    type Data = ShareComputationCircuitData;
    type Error = CircuitsErrors;

    fn compute(preset: Self::Preset, data: &Self::Data) -> Result<Self, CircuitsErrors> {
        let (threshold_params, _) =
            build_pair_for_preset(preset).map_err(|e| CircuitsErrors::Sample(e.to_string()))?;

        let moduli = threshold_params.moduli().to_vec();
        let l = moduli.len();
        let bounds = Bounds::compute(preset, data)?;
        let bits = Bits::compute(preset, &bounds)?;

        Ok(Configs {
            n: threshold_params.degree(),
            l,
            chunk_size: SHARE_COMPUTATION_CHUNK_SIZE,
            n_chunks: chunk_count(threshold_params.degree(), SHARE_COMPUTATION_CHUNK_SIZE),
            chunks_per_batch: chunks_per_batch(threshold_params.degree()),
            n_batches: batch_count(
                chunk_count(threshold_params.degree(), SHARE_COMPUTATION_CHUNK_SIZE),
                chunks_per_batch(threshold_params.degree()),
            ),
            moduli,
            bits,
            bounds,
        })
    }
}

impl Computation for Bits {
    type Preset = BfvPreset;
    type Data = Bounds;
    type Error = crate::utils::ZkHelpersUtilsError;

    fn compute(preset: Self::Preset, data: &Self::Data) -> Result<Self, Self::Error> {
        let (threshold_params, _) = build_pair_for_preset(preset)
            .map_err(|e| crate::utils::ZkHelpersUtilsError::ParseBound(e.to_string()))?;

        let mut bit_share = 0;
        for &qi in threshold_params.moduli() {
            let share_bound = BigUint::from(qi - 1);
            let bit_width = calculate_bit_width(BigInt::from(share_bound));
            bit_share = bit_share.max(bit_width);
        }

        Ok(Bits {
            bit_sk_secret: calculate_bit_width(BigInt::from(data.sk_bound.clone())),
            bit_e_sm_secret: calculate_bit_width(BigInt::from(data.e_sm_bound.clone())),
            bit_share,
        })
    }
}

impl Computation for Bounds {
    type Preset = BfvPreset;
    type Data = ShareComputationCircuitData;
    type Error = CircuitsErrors;

    fn compute(preset: Self::Preset, data: &Self::Data) -> Result<Self, Self::Error> {
        let (threshold_params, _) =
            build_pair_for_preset(preset).map_err(|e| CircuitsErrors::Sample(e.to_string()))?;
        let defaults = preset
            .search_defaults()
            .ok_or_else(|| CircuitsErrors::Sample("missing search defaults".to_string()))?;
        let num_ciphertexts = defaults.z;
        // Lambda is secure or insecure depending on the preset's security tier.
        let lambda = preset
            .lambda()
            .map_err(|e| CircuitsErrors::Sample(e.to_string()))?;

        // Use the same committee size as C1 (pk_generation) so smudging bounds and
        // bit widths match PK_GENERATION_BIT_E_SM / SHARE_COMPUTATION_E_SM_BIT_SECRET.
        let e_sm_config = SmudgingBoundCalculatorConfig::new(
            threshold_params,
            data.n_parties as usize,
            num_ciphertexts as usize,
            lambda,
        );

        let e_sm_calculator = SmudgingBoundCalculator::new(e_sm_config);

        let e_sm_bound = e_sm_calculator.calculate_sm_bound()?;

        Ok(Bounds {
            sk_bound: BigUint::from(SecretKey::sk_bound() as u128),
            e_sm_bound,
        })
    }
}

impl Computation for Inputs {
    type Preset = BfvPreset;
    type Data = ShareComputationCircuitData;
    type Error = CircuitsErrors;

    fn compute(preset: Self::Preset, data: &Self::Data) -> Result<Self, Self::Error> {
        let (threshold_params, _) =
            build_pair_for_preset(preset).map_err(|e| CircuitsErrors::Sample(e.to_string()))?;
        let moduli = threshold_params.moduli();
        let degree = threshold_params.degree();
        let num_moduli = moduli.len();
        let n_parties = data.n_parties as usize;

        let mut secret_crt = data.secret.clone();
        let sss = &data.secret_sss;

        if data.dkg_input_type == DkgInputType::SmudgingNoise {
            // Normalize secret_crt to [0, q_j) per limb so it matches what we put in y and what the circuit expects (e_sm_secret[j][i] == y[i][j][0]).
            secret_crt
                .reduce(moduli)
                .map_err(|e| CircuitsErrors::Sample(format!("secret_crt reduce: {:?}", e)))?;
        }

        // y[coeff_idx][mod_idx][0] = secret_crt[mod_idx][coeff_idx] (already in [0, q_j)); y[coeff_idx][mod_idx][1+party] = share in [0, q_j).
        let mut y: Vec<Vec<Vec<BigInt>>> = Vec::with_capacity(degree);
        for coeff_idx in 0..degree {
            let mut y_coeff: Vec<Vec<BigInt>> = Vec::with_capacity(num_moduli);
            for mod_idx in 0..num_moduli {
                let q_j = BigInt::from(moduli[mod_idx]);
                let mut y_mod: Vec<BigInt> = Vec::with_capacity(1 + n_parties);
                y_mod.push(secret_crt.limb(mod_idx).coefficients()[coeff_idx].clone());
                for party_idx in 0..n_parties {
                    let share_value = &sss[mod_idx][[party_idx, coeff_idx]];
                    y_mod.push(reduce(share_value, &q_j));
                }
                y_coeff.push(y_mod);
            }
            y.push(y_coeff);
        }

        let bounds = Bounds::compute(preset, data)?;
        let bits = Bits::compute(preset, &bounds)?;
        // The chunk-root commitment uses the same reverse and center transforms as C1.
        let expected_secret_commitment = match data.dkg_input_type {
            DkgInputType::SecretKey => compute_sc_sk_secret_root_commitment(
                secret_crt.limb(0),
                degree,
                SHARE_COMPUTATION_CHUNK_SIZE,
                bits.bit_sk_secret,
            ),
            DkgInputType::SmudgingNoise => compute_sc_esm_secret_root_commitment(
                &secret_crt,
                degree,
                SHARE_COMPUTATION_CHUNK_SIZE,
                moduli,
                bits.bit_e_sm_secret,
            ),
        };

        Ok(Inputs {
            secret_crt,
            y,
            expected_secret_commitment,
            dkg_input_type: data.dkg_input_type,
        })
    }

    // Used as input for Nargo execution.
    fn to_json(&self) -> serde_json::Result<serde_json::Value> {
        let y = bigint_3d_to_json_values(&self.y);
        let expected_secret_commitment = self.expected_secret_commitment.to_string();

        let (key, value) = match self.dkg_input_type {
            DkgInputType::SecretKey => (
                "sk_secret",
                poly_coefficients_to_toml_json(self.secret_crt.limb(0).coefficients()),
            ),
            DkgInputType::SmudgingNoise => (
                "e_sm_secret",
                serde_json::Value::Array(crt_polynomial_to_toml_json(&self.secret_crt)),
            ),
        };

        let mut json = serde_json::json!({
            "y": y,
            "expected_secret_commitment": expected_secret_commitment,
        });

        json.as_object_mut().unwrap().insert(key.to_string(), value);

        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ciphernodes_committee::CiphernodesCommitteeSize;
    use crate::computation::DkgInputType;
    use crate::dkg::share_computation::ShareComputationCircuitData;
    use e3_fhe_params::BfvPreset;

    #[test]
    fn test_bound_and_bits_computation_consistency() {
        let committee = CiphernodesCommitteeSize::Small.values();
        let sample = ShareComputationCircuitData::generate_sample(
            BfvPreset::InsecureThreshold512,
            committee,
            DkgInputType::SecretKey,
        )
        .unwrap();
        let bounds = Bounds::compute(BfvPreset::InsecureThreshold512, &sample).unwrap();
        let bits = Bits::compute(BfvPreset::InsecureThreshold512, &bounds).unwrap();
        let expected_sk_bits = calculate_bit_width(BigInt::from(bounds.sk_bound.clone()));

        assert_eq!(bits.bit_sk_secret, expected_sk_bits);
    }

    #[test]
    fn test_input_smudging_noise_secret_consistency() {
        let committee = CiphernodesCommitteeSize::Small.values();
        let sample = ShareComputationCircuitData::generate_sample(
            BfvPreset::InsecureThreshold512,
            committee,
            DkgInputType::SmudgingNoise,
        )
        .unwrap();
        let inputs = Inputs::compute(BfvPreset::InsecureThreshold512, &sample).unwrap();
        let degree = inputs.secret_crt.limb(0).coefficients().len();
        let num_moduli = inputs.secret_crt.limbs.len();
        for coeff_idx in 0..degree {
            for mod_idx in 0..num_moduli {
                let secret_coeff =
                    inputs.secret_crt.limb(mod_idx).coefficients()[coeff_idx].clone();
                let y_secret = inputs.y[coeff_idx][mod_idx][0].clone();
                assert_eq!(
                    secret_coeff, y_secret,
                    "secret consistency: secret_crt[{mod_idx}][{coeff_idx}] must equal y[{coeff_idx}][{mod_idx}][0]"
                );
            }
        }
    }

    #[test]
    fn test_constants_json_roundtrip() {
        let committee = CiphernodesCommitteeSize::Small.values();
        let sample = ShareComputationCircuitData::generate_sample(
            BfvPreset::InsecureThreshold512,
            committee,
            DkgInputType::SecretKey,
        )
        .unwrap();

        let constants = Configs::compute(BfvPreset::InsecureThreshold512, &sample).unwrap();

        let json = constants.to_json().unwrap();
        let decoded: Configs = serde_json::from_value(json).unwrap();

        assert_eq!(decoded.n, constants.n);
        assert_eq!(decoded.l, constants.l);
        assert_eq!(decoded.moduli, constants.moduli);
        assert_eq!(decoded.bits, constants.bits);
        assert_eq!(decoded.bounds, constants.bounds);
    }

    #[test]
    fn test_chunk_dimensions_use_ceil_division() {
        assert_eq!(chunk_count(512, 512), 1);
        assert_eq!(chunk_count(513, 512), 2);
        assert_eq!(batch_count(5, 4), 2);
        assert_eq!(chunks_per_batch(512), 1);
        assert_eq!(chunks_per_batch(8192), 4);
    }

    #[test]
    fn test_inputs_split_into_fixed_width_chunks() {
        let committee = CiphernodesCommitteeSize::Small.values();
        let sample = ShareComputationCircuitData::generate_sample(
            BfvPreset::InsecureThreshold512,
            committee,
            DkgInputType::SecretKey,
        )
        .unwrap();
        let inputs = Inputs::compute(BfvPreset::InsecureThreshold512, &sample).unwrap();
        let chunks = inputs.split_into_chunks(128).unwrap();

        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].chunk_idx, 0);
        assert_eq!(chunks[3].chunk_idx, 3);
        assert_eq!(chunks[0].y_chunk.len(), 128);
        assert_eq!(chunks[0].secret_crt.limb(0).coefficients().len(), 128);
        assert_eq!(chunks[0].y_chunk[0], inputs.y[0]);
        assert_eq!(
            chunks[3].secret_crt.limb(0).coefficients()[0],
            inputs.secret_crt.limb(0).coefficients()[384]
        );
    }
}
