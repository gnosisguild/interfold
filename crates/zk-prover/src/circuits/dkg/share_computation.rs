// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::circuits::utils::{
    bytes_to_field_strings, honk_proof_bytes_to_field_strings, inputs_json_to_input_map,
    zk_proof_bytes_to_field_strings,
};
use crate::circuits::vk;
use crate::error::ZkError;
use crate::prover::ZkProver;
use crate::traits::Provable;
use crate::witness::{CompiledCircuit, WitnessGenerator};
use e3_events::CircuitName;
use e3_events::{CircuitVariant, Proof};
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::circuits::dkg::share_computation::{
    ChunkInputs, Inputs, ShareComputationCircuit, ShareComputationCircuitData,
    SHARE_COMPUTATION_CHUNK_SIZE,
};
use e3_zk_helpers::computation::{Computation, DkgInputType};
use e3_zk_helpers::dkg::share_computation::{batch_count, chunk_count, chunks_per_batch};
use serde::Serialize;

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

#[allow(dead_code)]
#[derive(Serialize)]
struct ChunkBatchWitness {
    chunk_vk: Vec<String>,
    chunk_proofs: Vec<Vec<String>>,
    chunk_public_inputs: Vec<Vec<String>>,
    chunk_key_hash: String,
    batch_idx: u32,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct ShareComputationFinalWitness {
    batch_vk: Vec<String>,
    batch_proofs: Vec<Vec<String>>,
    batch_public_inputs: Vec<Vec<String>>,
    batch_key_hash: String,
}

fn final_circuit(input_type: DkgInputType) -> CircuitName {
    match input_type {
        DkgInputType::SecretKey => CircuitName::SkShareComputationFinal,
        DkgInputType::SmudgingNoise => CircuitName::ESmShareComputationFinal,
    }
}

#[allow(dead_code)]
fn load_circuit_and_witness<T: Serialize>(
    prover: &ZkProver,
    circuit: CircuitName,
    variant: CircuitVariant,
    input: &T,
    artifacts_dir: &str,
) -> Result<Vec<u8>, ZkError> {
    let path = prover
        .circuits_dir(variant, artifacts_dir)
        .join(circuit.dir_path())
        .join(format!("{}.json", circuit.as_str()));
    let compiled = CompiledCircuit::from_file(&path)?;
    let json = serde_json::to_value(input)
        .map_err(|error| ZkError::SerializationError(error.to_string()))?;
    let input_map = inputs_json_to_input_map(&json)?;
    WitnessGenerator::new().generate_witness(&compiled, input_map)
}

#[allow(dead_code)]
fn prove_chunk(
    prover: &ZkProver,
    chunk: &ChunkInputs,
    input_type: DkgInputType,
    e3_id: &str,
    artifacts_dir: &str,
) -> Result<Proof, ZkError> {
    let circuit = match input_type {
        DkgInputType::SecretKey => CircuitName::SkShareComputationChunk,
        DkgInputType::SmudgingNoise => CircuitName::ESmShareComputationChunk,
    };
    let witness = load_circuit_and_witness(
        prover,
        circuit,
        CircuitVariant::Recursive,
        chunk,
        artifacts_dir,
    )?;
    prover.generate_proof_with_variant(
        circuit,
        &witness,
        e3_id,
        CircuitVariant::Recursive,
        artifacts_dir,
    )
}

/// Prove the chunked C2 pipeline without changing the legacy C2 request route.
///
/// The caller can switch to this pipeline after C1/C3/C4 commitment links consume
/// the chunk-root format. The legacy [`Provable`] implementation remains available
/// during that migration.
#[allow(dead_code)]
pub fn prove_chunked_share_computation(
    prover: &ZkProver,
    preset: BfvPreset,
    data: &ShareComputationCircuitData,
    e3_id: &str,
    artifacts_dir: &str,
) -> Result<Proof, ZkError> {
    let inputs = Inputs::compute(preset, data)
        .map_err(|error| ZkError::InputsGenerationFailed(error.to_string()))?;
    let chunks = inputs
        .split_into_chunks(SHARE_COMPUTATION_CHUNK_SIZE)
        .map_err(|error| ZkError::InputsGenerationFailed(error.to_string()))?;
    let n_chunks = chunk_count(inputs.y.len(), SHARE_COMPUTATION_CHUNK_SIZE);
    let chunks_per_batch = chunks_per_batch(inputs.y.len());
    let n_batches = batch_count(n_chunks, chunks_per_batch);
    if n_chunks % chunks_per_batch != 0 {
        return Err(ZkError::InvalidInput(format!(
            "chunk count {} is not divisible by batch width {}",
            n_chunks, chunks_per_batch
        )));
    }
    if chunks.len() != n_chunks {
        return Err(ZkError::InvalidInput(format!(
            "chunk count mismatch: generated {}, expected {}",
            chunks.len(),
            n_chunks
        )));
    }

    let chunk_vk = vk::load_vk_artifacts(
        &prover.circuits_dir(CircuitVariant::Recursive, artifacts_dir),
        match data.dkg_input_type {
            DkgInputType::SecretKey => CircuitName::SkShareComputationChunk,
            DkgInputType::SmudgingNoise => CircuitName::ESmShareComputationChunk,
        },
    )?;
    let mut chunk_proofs = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        chunk_proofs.push(prove_chunk(
            prover,
            chunk,
            data.dkg_input_type,
            &format!("{e3_id}-c2-chunk-{}", chunk.chunk_idx),
            artifacts_dir,
        )?);
    }

    let batch_vk = vk::load_vk_artifacts(
        &prover.circuits_dir(CircuitVariant::Default, artifacts_dir),
        CircuitName::ShareComputationChunkBatch,
    )?;
    let mut batch_proofs = Vec::with_capacity(n_batches);
    for batch_idx in 0..n_batches {
        let start = batch_idx * chunks_per_batch;
        let end = start + chunks_per_batch;
        let batch_chunks = &chunk_proofs[start..end];
        let witness = ChunkBatchWitness {
            chunk_vk: chunk_vk.verification_key.clone(),
            chunk_proofs: batch_chunks
                .iter()
                .map(|proof| zk_proof_bytes_to_field_strings(proof.data.as_ref()))
                .collect::<Result<_, _>>()?,
            chunk_public_inputs: batch_chunks
                .iter()
                .map(|proof| bytes_to_field_strings(proof.public_signals.as_ref()))
                .collect::<Result<_, _>>()?,
            chunk_key_hash: chunk_vk.key_hash.clone(),
            batch_idx: batch_idx as u32,
        };
        let witness = load_circuit_and_witness(
            prover,
            CircuitName::ShareComputationChunkBatch,
            CircuitVariant::Default,
            &witness,
            artifacts_dir,
        )?;
        batch_proofs.push(prover.generate_recursive_aggregation_bin_proof(
            CircuitName::ShareComputationChunkBatch,
            &witness,
            &format!("{e3_id}-c2-batch-{batch_idx}"),
            artifacts_dir,
        )?);
    }

    let witness = ShareComputationFinalWitness {
        batch_vk: batch_vk.verification_key,
        batch_proofs: batch_proofs
            .iter()
            .map(|proof| honk_proof_bytes_to_field_strings(proof.data.as_ref()))
            .collect::<Result<_, _>>()?,
        batch_public_inputs: batch_proofs
            .iter()
            .map(|proof| bytes_to_field_strings(proof.public_signals.as_ref()))
            .collect::<Result<_, _>>()?,
        batch_key_hash: batch_vk.key_hash,
    };
    let final_circuit = final_circuit(data.dkg_input_type);
    let witness = load_circuit_and_witness(
        prover,
        final_circuit,
        CircuitVariant::Recursive,
        &witness,
        artifacts_dir,
    )?;
    prover.generate_proof_with_variant(
        final_circuit,
        &witness,
        &format!("{e3_id}-c2-final"),
        CircuitVariant::Recursive,
        artifacts_dir,
    )
}
