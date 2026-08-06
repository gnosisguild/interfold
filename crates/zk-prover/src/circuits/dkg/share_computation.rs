// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::circuits::aggregation::c2_chunk_accumulator::{
    finalize_c2_chunk_fold, generate_sequential_c2_chunk_fold,
};
use crate::circuits::utils::{
    bytes_to_field_strings, inputs_json_to_input_map, prove_recursive_circuit,
};
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

pub const DEFAULT_C2_CHUNK_SIZE: usize = 512;

pub struct ChunkedShareComputationProofs {
    pub proof: e3_events::Proof,
    pub chunk_count: usize,
}

/// Generate the terminal C2 proof from one base proof and all deterministic coefficient chunks.
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
    let inputs = Inputs::compute(preset, data)
        .map_err(|e| ZkError::InputsGenerationFailed(e.to_string()))?;
    let base_json = inputs
        .to_json()
        .map_err(|e| ZkError::SerializationError(e.to_string()))?;
    let degree = preset
        .threshold_counterpart()
        .unwrap_or(preset)
        .metadata()
        .degree;
    if chunk_size == 0 || degree == 0 || degree % chunk_size != 0 {
        return Err(ZkError::InvalidInput(format!(
            "C2 chunk size {chunk_size} must divide polynomial degree {degree}"
        )));
    }
    let chunk_count = degree / chunk_size;
    let base_circuit = match data.dkg_input_type {
        DkgInputType::SecretKey => CircuitName::SkShareComputationBase,
        DkgInputType::SmudgingNoise => CircuitName::ESmShareComputationBase,
    };
    let base = prove_recursive_circuit(
        prover,
        base_circuit,
        &base_json,
        &format!("{e3_id}-c2-base"),
        artifacts_dir,
    )?;
    let base_public = bytes_to_field_strings(base.public_signals.as_ref())?;
    if base_public.len() < 1 + chunk_count {
        return Err(ZkError::InvalidInput(format!(
            "C2 base proof has {} public fields, expected at least {}",
            base_public.len(),
            1 + chunk_count
        )));
    }
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
    let mut indices = Vec::with_capacity(chunk_count);
    for chunk_idx in 0..chunk_count {
        let start = chunk_idx * chunk_size;
        let mut chunk_json = serde_json::Map::new();
        chunk_json.insert(
            "chunk_commitment".into(),
            Value::String(base_public[1 + chunk_idx].clone()),
        );
        chunk_json.insert("chunk_idx".into(), Value::from(chunk_idx as u64));
        chunk_json.insert(
            "y_chunk".into(),
            Value::Array(y[start..start + chunk_size].to_vec()),
        );
        let input_map = inputs_json_to_input_map(&Value::Object(chunk_json))?;
        let circuit_path = prover
            .circuits_dir(e3_events::CircuitVariant::Recursive, artifacts_dir)
            .join(CircuitName::ShareComputationChunk.dir_path())
            .join(format!(
                "{}.json",
                CircuitName::ShareComputationChunk.as_str()
            ));
        let compiled = crate::witness::CompiledCircuit::from_file(&circuit_path)?;
        let witness =
            crate::witness::WitnessGenerator::new().generate_witness(&compiled, input_map)?;
        let proof = prover.generate_proof_with_variant(
            CircuitName::ShareComputationChunk,
            &witness,
            &format!("{e3_id}-c2-chunk-{chunk_idx}"),
            e3_events::CircuitVariant::Recursive,
            artifacts_dir,
        )?;
        chunks.push(proof);
        indices.push(chunk_idx as u32);
    }

    let accumulator = generate_sequential_c2_chunk_fold(
        prover,
        &base,
        &chunks,
        &indices,
        chunk_count,
        e3_id,
        artifacts_dir,
    )?;
    let finalizer_circuit = match data.dkg_input_type {
        DkgInputType::SecretKey => CircuitName::SkC2ChunkFinalize,
        DkgInputType::SmudgingNoise => CircuitName::ESmC2ChunkFinalize,
    };
    let proof = finalize_c2_chunk_fold(
        prover,
        &accumulator,
        chunk_count,
        finalizer_circuit,
        e3_id,
        artifacts_dir,
    )?;
    Ok(ChunkedShareComputationProofs { proof, chunk_count })
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
