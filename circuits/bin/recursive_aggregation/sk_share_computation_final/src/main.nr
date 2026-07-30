// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Verifies all SK chunk batches and reconstructs the C2 public output shape.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::dkg::{
    L_THRESHOLD, N, SHARE_COMPUTATION_CHUNK_SIZE as CHUNK_SIZE,
    SHARE_COMPUTATION_CHUNKS_PER_BATCH as CHUNKS_PER_BATCH,
    SHARE_COMPUTATION_N_BATCHES as N_BATCHES, SHARE_COMPUTATION_N_CHUNKS as N_CHUNKS,
};
use lib::configs::default::N_PARTIES;
use lib::math::commitments::{
    compute_recursive_aggregation_commitment, compute_sc_party_share_chunk_root_commitment,
    compute_sc_secret_chunk_root_commitment, compute_vk_hash,
};

pub global CHUNK_COMMITMENT_LEN: u32 = 1 + (N_PARTIES * L_THRESHOLD);
pub global BATCH_RETURN_LEN: u32 = 1 + (CHUNKS_PER_BATCH * CHUNK_COMMITMENT_LEN);
pub global BATCH_PUBLIC_LEN: u32 = 2 + BATCH_RETURN_LEN;

fn main(
    batch_vk: UltraHonkVerificationKey,
    batch_proofs: [UltraHonkProof; N_BATCHES],
    batch_public_inputs: [[Field; BATCH_PUBLIC_LEN]; N_BATCHES],
    batch_key_hash: pub Field,
) -> pub (Field, Field, [[Field; L_THRESHOLD]; N_PARTIES]) {
    assert(N_BATCHES * CHUNKS_PER_BATCH == N_CHUNKS);

    for i in 0..N_BATCHES {
        verify_honk_proof_non_zk(
            batch_vk,
            batch_proofs[i],
            batch_public_inputs[i],
            batch_key_hash,
        );
        assert(batch_public_inputs[i][1] == i as Field);
        assert(batch_public_inputs[i][0] == batch_public_inputs[0][0]);

        let mut batch_payload = [batch_public_inputs[i][1]].as_vector();
        for j in 0..CHUNKS_PER_BATCH {
            let chunk_idx = i * CHUNKS_PER_BATCH + j;
            batch_payload = batch_payload.push_back(chunk_idx as Field);
            for k in 0..CHUNK_COMMITMENT_LEN {
                batch_payload = batch_payload.push_back(
                    batch_public_inputs[i][3 + j * CHUNK_COMMITMENT_LEN + k],
                );
            }
        }
        assert(
            batch_public_inputs[i][2] == compute_recursive_aggregation_commitment(batch_payload),
        );
    }

    let vk_lineage = compute_vk_hash([batch_public_inputs[0][0], batch_key_hash].as_vector());
    let mut secret_chunks: [Field; N_CHUNKS] = [0; N_CHUNKS];
    let mut share_chunks: [[Field; N_CHUNKS]; N_PARTIES * L_THRESHOLD] =
        [[0; N_CHUNKS]; N_PARTIES * L_THRESHOLD];
    for batch_idx in 0..N_BATCHES {
        for chunk_in_batch in 0..CHUNKS_PER_BATCH {
            let chunk_idx = batch_idx * CHUNKS_PER_BATCH + chunk_in_batch;
            let start = 3 + chunk_in_batch * CHUNK_COMMITMENT_LEN;
            secret_chunks[chunk_idx] = batch_public_inputs[batch_idx][start];
            for slot in 0..(N_PARTIES * L_THRESHOLD) {
                share_chunks[slot][chunk_idx] = batch_public_inputs[batch_idx][start + 1 + slot];
            }
        }
    }

    let secret_root = compute_sc_secret_chunk_root_commitment(0, N, CHUNK_SIZE, secret_chunks);
    let mut share_roots: [[Field; L_THRESHOLD]; N_PARTIES] = [[0; L_THRESHOLD]; N_PARTIES];
    for party_idx in 0..N_PARTIES {
        for mod_idx in 0..L_THRESHOLD {
            let slot = party_idx * L_THRESHOLD + mod_idx;
            share_roots[party_idx][mod_idx] = compute_sc_party_share_chunk_root_commitment(
                party_idx,
                mod_idx,
                N,
                CHUNK_SIZE,
                share_chunks[slot],
            );
        }
    }

    (vk_lineage, secret_root, share_roots)
}
