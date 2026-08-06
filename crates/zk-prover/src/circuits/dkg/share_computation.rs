// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::circuits::aggregation::c2_chunk_batch::{
    finalize_c2_chunk_batches, generate_c2_chunk_batches,
};
use crate::circuits::aggregation::c2_chunk_config::{
    chunks_per_batch, compiled_batch_count, compiled_chunk_count,
};
use crate::circuits::utils::inputs_json_to_input_map;
use crate::error::ZkError;
use crate::prover::ZkProver;
use crate::traits::Provable;
use e3_events::CircuitName;
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::computation::{Computation, DkgInputType};
use e3_zk_helpers::dkg::share_computation::{
    Inputs, ShareComputationCircuit, ShareComputationCircuitData,
};
use serde_json::Value;

pub use crate::circuits::aggregation::c2_chunk_config::DEFAULT_C2_CHUNK_SIZE;

fn validate_c2_chunk_layout(degree: usize, chunk_size: usize) -> Result<(usize, usize), ZkError> {
    if chunk_size == 0 || degree == 0 || degree % chunk_size != 0 {
        return Err(ZkError::InvalidInput(format!(
            "C2 chunk size {chunk_size} must divide polynomial degree {degree}"
        )));
    }
    let chunk_count = degree / chunk_size;
    let expected_chunk_count = compiled_chunk_count(degree);
    if chunk_count != expected_chunk_count {
        return Err(ZkError::InvalidInput(format!(
            "C2 chunk size {chunk_size} produces {chunk_count} chunks, but the selected artifacts require {expected_chunk_count}"
        )));
    }
    let batch_count = chunk_count / chunks_per_batch(degree);
    let expected_batch_count = compiled_batch_count(degree);
    if batch_count != expected_batch_count {
        return Err(ZkError::InvalidInput(format!(
            "C2 chunk size {chunk_size} produces {batch_count} batches, but the selected artifacts require {expected_batch_count}"
        )));
    }
    Ok((chunk_count, batch_count))
}

pub struct ChunkedShareComputationProofs {
    pub proof: e3_events::Proof,
    pub chunk_count: usize,
}

/// Generate the terminal C2 proof from all deterministic coefficient chunks.
pub fn prove_chunked_share_computation(
    prover: &ZkProver,
    preset: BfvPreset,
    data: &ShareComputationCircuitData,
    e3_id: &str,
    artifacts_dir: &str,
) -> Result<ChunkedShareComputationProofs, ZkError> {
    prove_chunked_share_computation_with_chunk_size(
        prover,
        preset,
        data,
        e3_id,
        artifacts_dir,
        DEFAULT_C2_CHUNK_SIZE,
    )
}

pub fn prove_chunked_share_computation_with_chunk_size(
    prover: &ZkProver,
    preset: BfvPreset,
    data: &ShareComputationCircuitData,
    e3_id: &str,
    artifacts_dir: &str,
    chunk_size: usize,
) -> Result<ChunkedShareComputationProofs, ZkError> {
    let degree = preset
        .threshold_counterpart()
        .unwrap_or(preset)
        .metadata()
        .degree;
    let (chunk_count, _batch_count) = validate_c2_chunk_layout(degree, chunk_size)?;
    let inputs = Inputs::compute(preset, data)
        .map_err(|e| ZkError::InputsGenerationFailed(e.to_string()))?;
    let base_json = inputs
        .to_json()
        .map_err(|e| ZkError::SerializationError(e.to_string()))?;
    let chunk_circuit = match data.dkg_input_type {
        DkgInputType::SecretKey => CircuitName::SkShareComputationChunk,
        DkgInputType::SmudgingNoise => CircuitName::ESmShareComputationChunk,
    };
    let y = base_json
        .get("y")
        .and_then(Value::as_array)
        .ok_or_else(|| ZkError::SerializationError("C2 input is missing y".into()))?;
    if y.len() != degree {
        return Err(ZkError::InvalidInput(format!(
            "C2 y has {} coefficients, expected {degree}",
            y.len()
        )));
    }

    let mut chunks = Vec::with_capacity(chunk_count);
    for chunk_idx in 0..chunk_count {
        let start = chunk_idx * chunk_size;
        let mut chunk_json = serde_json::Map::new();
        chunk_json.insert("chunk_idx".into(), Value::from(chunk_idx as u64));
        let secret_key = match data.dkg_input_type {
            DkgInputType::SecretKey => "sk_secret",
            DkgInputType::SmudgingNoise => "e_sm_secret",
        };
        let secret = base_json.get(secret_key).ok_or_else(|| {
            ZkError::SerializationError(format!("C2 input is missing {secret_key}"))
        })?;
        let secret_chunk = if data.dkg_input_type == DkgInputType::SecretKey {
            let coefficients = secret
                .as_object()
                .and_then(|object| object.get("coefficients"))
                .and_then(Value::as_array)
                .ok_or_else(|| ZkError::SerializationError("SK secret JSON is malformed".into()))?;
            Value::Object(
                [(
                    "coefficients".into(),
                    Value::Array(coefficients[start..start + chunk_size].to_vec()),
                )]
                .into_iter()
                .collect(),
            )
        } else {
            let limbs = secret.as_array().ok_or_else(|| {
                ZkError::SerializationError("ESM secret JSON must contain CRT limbs".into())
            })?;
            Value::Array(
                limbs
                    .iter()
                    .map(|limb| {
                        limb.as_object()
                            .and_then(|object| object.get("coefficients"))
                            .and_then(Value::as_array)
                            .map(|values| {
                                Value::Object(
                                    [(
                                        "coefficients".into(),
                                        Value::Array(values[start..start + chunk_size].to_vec()),
                                    )]
                                    .into_iter()
                                    .collect(),
                                )
                            })
                            .ok_or_else(|| {
                                ZkError::SerializationError(
                                    "ESM secret JSON must contain CRT limbs".into(),
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        };
        chunk_json.insert("secret_chunk".into(), secret_chunk);
        chunk_json.insert(
            "y_chunk".into(),
            Value::Array(y[start..start + chunk_size].to_vec()),
        );
        let input_map = inputs_json_to_input_map(&Value::Object(chunk_json))?;
        let circuit_path = prover
            .circuits_dir(e3_events::CircuitVariant::Recursive, artifacts_dir)
            .join(chunk_circuit.dir_path())
            .join(format!("{}.json", chunk_circuit.as_str()));
        let compiled = crate::witness::CompiledCircuit::from_file(&circuit_path)?;
        let witness = crate::witness::WitnessGenerator::new()
            .generate_witness(&compiled, input_map)
            .map_err(|error| {
                ZkError::WitnessGenerationFailed(format!("C2 chunk {chunk_idx} witness: {error}"))
            })?;
        let proof = prover.generate_proof_with_variant(
            chunk_circuit,
            &witness,
            &format!("{e3_id}-c2-chunk-{chunk_idx}"),
            e3_events::CircuitVariant::Recursive,
            artifacts_dir,
        )?;
        chunks.push(proof);
    }

    let batches = generate_c2_chunk_batches(
        prover,
        chunk_circuit,
        &chunks,
        chunk_count,
        degree,
        e3_id,
        artifacts_dir,
    )?;
    let finalizer_circuit = match data.dkg_input_type {
        DkgInputType::SecretKey => CircuitName::SkC2ChunkFinalize,
        DkgInputType::SmudgingNoise => CircuitName::ESmC2ChunkFinalize,
    };
    let proof =
        finalize_c2_chunk_batches(prover, &batches, finalizer_circuit, e3_id, artifacts_dir)?;
    Ok(ChunkedShareComputationProofs { proof, chunk_count })
}

#[cfg(test)]
mod tests {
    use super::validate_c2_chunk_layout;

    #[test]
    fn rejects_chunk_size_with_a_different_compiled_chunk_count() {
        assert!(validate_c2_chunk_layout(8192, 256).is_err());
    }
}

impl Provable for ShareComputationCircuit {
    type Params = BfvPreset;
    type Input = ShareComputationCircuitData;
    type Inputs = Inputs;

    fn resolve_circuit_name(&self, _params: &Self::Params, input: &Self::Input) -> CircuitName {
        match input.dkg_input_type {
            DkgInputType::SecretKey => CircuitName::SkShareComputation,
            DkgInputType::SmudgingNoise => CircuitName::ESmShareComputation,
        }
    }

    fn valid_circuits(&self) -> Vec<CircuitName> {
        vec![
            CircuitName::SkShareComputation,
            CircuitName::ESmShareComputation,
        ]
    }

    fn circuit(&self) -> CircuitName {
        CircuitName::SkShareComputation
    }
}
