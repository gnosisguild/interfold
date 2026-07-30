// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::dkg::{
    L_THRESHOLD, N, PARITY_MATRIX, SHARE_COMPUTATION_BIT_SHARE, SHARE_COMPUTATION_CHUNK_SIZE,
    SHARE_COMPUTATION_E_SM_BIT_SECRET, SHARE_COMPUTATION_E_SM_CONFIGS, SHARE_COMPUTATION_N_CHUNKS,
};
use lib::configs::default::{N_PARTIES, T};
use lib::core::dkg::share_computation_chunk::SmudgingNoiseShareComputationChunk;
use lib::math::polynomial::Polynomial;

pub global CHUNK_PUBLIC_LEN: u32 = 1 + (N_PARTIES * L_THRESHOLD);

fn main(
    chunk_idx: pub u32,
    secret_chunk: [Polynomial<SHARE_COMPUTATION_CHUNK_SIZE>; L_THRESHOLD],
    y_chunk: [[[Field; N_PARTIES + 1]; L_THRESHOLD]; SHARE_COMPUTATION_CHUNK_SIZE],
) -> pub [Field; CHUNK_PUBLIC_LEN] {
    assert(chunk_idx < SHARE_COMPUTATION_N_CHUNKS);

    let circuit: SmudgingNoiseShareComputationChunk<N, L_THRESHOLD, N_PARTIES, T, SHARE_COMPUTATION_E_SM_BIT_SECRET, SHARE_COMPUTATION_BIT_SHARE, SHARE_COMPUTATION_CHUNK_SIZE> = SmudgingNoiseShareComputationChunk::new(
        SHARE_COMPUTATION_E_SM_CONFIGS,
        secret_chunk,
        y_chunk,
        PARITY_MATRIX,
    );
    circuit.execute(chunk_idx)
}
