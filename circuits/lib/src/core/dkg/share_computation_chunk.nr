// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::core::dkg::share_computation::{check_range_bounds, Configs, verify_parity_check};
use crate::math::commitments::{
    compute_sc_esm_secret_chunk_commitment, compute_sc_party_share_chunk_commitment,
    compute_sc_secret_chunk_commitment,
};
use crate::math::polynomial::Polynomial;

/// The public commitment count for one chunk.
pub fn chunk_commitment_count<let L: u32, let N_PARTIES: u32>() -> u32 {
    1 + (N_PARTIES * L)
}

/// Verifies one SK share-computation chunk.
pub struct SecretKeyShareComputationChunk<let N: u32, let L: u32, let N_PARTIES: u32, let T: u32, let BIT_SECRET: u32, let BIT_SHARE: u32, let CHUNK_SIZE: u32> {
    configs: Configs<L>,
    secret_chunk: Polynomial<CHUNK_SIZE>,
    y_chunk: [[[Field; N_PARTIES + 1]; L]; CHUNK_SIZE],
    h: [[[Field; N_PARTIES + 1]; N_PARTIES - T]; L],
}

impl<let N: u32, let L: u32, let N_PARTIES: u32, let T: u32, let BIT_SECRET: u32, let BIT_SHARE: u32, let CHUNK_SIZE: u32> SecretKeyShareComputationChunk<N, L, N_PARTIES, T, BIT_SECRET, BIT_SHARE, CHUNK_SIZE> {
    pub fn new(
        configs: Configs<L>,
        secret_chunk: Polynomial<CHUNK_SIZE>,
        y_chunk: [[[Field; N_PARTIES + 1]; L]; CHUNK_SIZE],
        h: [[[Field; N_PARTIES + 1]; N_PARTIES - T]; L],
    ) -> Self {
        SecretKeyShareComputationChunk { configs, secret_chunk, y_chunk, h }
    }

    pub fn execute(self, chunk_idx: u32) -> [Field; 1 + (N_PARTIES * L)] {
        for coeff_idx in 0..CHUNK_SIZE {
            for mod_idx in 0..L {
                assert(
                    self.y_chunk[coeff_idx][mod_idx][0]
                        == self.secret_chunk.coefficients[coeff_idx],
                );
            }
        }

        check_range_bounds::<CHUNK_SIZE, L, N_PARTIES, BIT_SHARE>(self.configs.qis, self.y_chunk);
        verify_parity_check::<CHUNK_SIZE, L, N_PARTIES, T>(self.configs.qis, self.h, self.y_chunk);

        let mut out: [Field; 1 + (N_PARTIES * L)] = [0; 1 + (N_PARTIES * L)];
        out[0] = compute_sk_secret_chunk_commitment::<N, BIT_SECRET, CHUNK_SIZE>(
            self.secret_chunk,
            chunk_idx,
        );

        for party_idx in 0..N_PARTIES {
            for mod_idx in 0..L {
                out[1 + party_idx * L + mod_idx] = compute_share_chunk_commitment::<N, N_PARTIES, L, BIT_SHARE, CHUNK_SIZE>(
                    self.y_chunk,
                    party_idx,
                    mod_idx,
                    chunk_idx,
                );
            }
        }

        out
    }
}

/// Verifies one ESM share-computation chunk.
pub struct SmudgingNoiseShareComputationChunk<let N: u32, let L: u32, let N_PARTIES: u32, let T: u32, let BIT_SECRET: u32, let BIT_SHARE: u32, let CHUNK_SIZE: u32> {
    configs: Configs<L>,
    secret_chunk: [Polynomial<CHUNK_SIZE>; L],
    y_chunk: [[[Field; N_PARTIES + 1]; L]; CHUNK_SIZE],
    h: [[[Field; N_PARTIES + 1]; N_PARTIES - T]; L],
}

impl<let N: u32, let L: u32, let N_PARTIES: u32, let T: u32, let BIT_SECRET: u32, let BIT_SHARE: u32, let CHUNK_SIZE: u32> SmudgingNoiseShareComputationChunk<N, L, N_PARTIES, T, BIT_SECRET, BIT_SHARE, CHUNK_SIZE> {
    pub fn new(
        configs: Configs<L>,
        secret_chunk: [Polynomial<CHUNK_SIZE>; L],
        y_chunk: [[[Field; N_PARTIES + 1]; L]; CHUNK_SIZE],
        h: [[[Field; N_PARTIES + 1]; N_PARTIES - T]; L],
    ) -> Self {
        SmudgingNoiseShareComputationChunk { configs, secret_chunk, y_chunk, h }
    }

    pub fn execute(self, chunk_idx: u32) -> [Field; 1 + (N_PARTIES * L)] {
        for coeff_idx in 0..CHUNK_SIZE {
            for mod_idx in 0..L {
                assert(
                    self.y_chunk[coeff_idx][mod_idx][0]
                        == self.secret_chunk[mod_idx].coefficients[coeff_idx],
                );
            }
        }

        check_range_bounds::<CHUNK_SIZE, L, N_PARTIES, BIT_SHARE>(self.configs.qis, self.y_chunk);
        verify_parity_check::<CHUNK_SIZE, L, N_PARTIES, T>(self.configs.qis, self.h, self.y_chunk);

        let mut normalized: [Polynomial<CHUNK_SIZE>; L] = [Polynomial::new([0; CHUNK_SIZE]); L];
        for mod_idx in 0..L {
            let q = self.configs.qis[mod_idx];
            let half = (q - 1) / 2;
            let mut coefficients: [Field; CHUNK_SIZE] = [0; CHUNK_SIZE];
            for i in 0..CHUNK_SIZE {
                let coefficient = self.secret_chunk[mod_idx].coefficients[i];
                let centered = if (coefficient as u64) > (half as u64) {
                    coefficient - q
                } else {
                    coefficient
                };
                coefficients[CHUNK_SIZE - 1 - i] = centered;
            }
            normalized[mod_idx] = Polynomial::new(coefficients);
        }

        let mut out: [Field; 1 + (N_PARTIES * L)] = [0; 1 + (N_PARTIES * L)];
        out[0] = compute_sc_esm_secret_chunk_commitment::<CHUNK_SIZE, L, BIT_SECRET>(
            chunk_idx,
            N,
            normalized,
        );

        for party_idx in 0..N_PARTIES {
            for mod_idx in 0..L {
                out[1 + party_idx * L + mod_idx] = compute_share_chunk_commitment::<N, N_PARTIES, L, BIT_SHARE, CHUNK_SIZE>(
                    self.y_chunk,
                    party_idx,
                    mod_idx,
                    chunk_idx,
                );
            }
        }

        out
    }
}

fn compute_sk_secret_chunk_commitment<let N: u32, let BIT: u32, let CHUNK_SIZE: u32>(
    secret_chunk: Polynomial<CHUNK_SIZE>,
    chunk_idx: u32,
) -> Field {
    let mut reversed: [Field; CHUNK_SIZE] = [0; CHUNK_SIZE];
    for i in 0..CHUNK_SIZE {
        reversed[CHUNK_SIZE - 1 - i] = secret_chunk.coefficients[i];
    }
    compute_sc_secret_chunk_commitment::<CHUNK_SIZE, BIT>(
        0,
        chunk_idx,
        N,
        Polynomial::new(reversed),
    )
}

fn compute_share_chunk_commitment<let N: u32, let N_PARTIES: u32, let L: u32, let BIT: u32, let CHUNK_SIZE: u32>(
    y_chunk: [[[Field; N_PARTIES + 1]; L]; CHUNK_SIZE],
    party_idx: u32,
    mod_idx: u32,
    chunk_idx: u32,
) -> Field {
    let mut reversed: [Field; CHUNK_SIZE] = [0; CHUNK_SIZE];
    for i in 0..CHUNK_SIZE {
        reversed[CHUNK_SIZE - 1 - i] = y_chunk[i][mod_idx][party_idx + 1];
    }
    compute_sc_party_share_chunk_commitment::<CHUNK_SIZE, BIT>(
        party_idx,
        mod_idx,
        chunk_idx,
        N,
        Polynomial::new(reversed),
    )
}
