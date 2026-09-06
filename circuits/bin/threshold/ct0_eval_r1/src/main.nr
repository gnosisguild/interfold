// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group B (P3): re-supplies `r1is`, re-derives `r1_commit` for binding against Round A,
//! and evaluates the per-limb terms the final ct0 identity needs.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_R1};
use lib::math::commitments::{compute_ude_multi_chunk_commitment, UDE_KIND_CT0_R1};
use lib::math::polynomial::Polynomial;

fn main(r1is: [Polynomial<(2 * N) - 1>; L], gamma: pub Field) -> pub (Field, [Field; L]) {
    let r1_commit = compute_ude_multi_chunk_commitment::<(2 * N) - 1, L, USER_DATA_ENCRYPTION_BIT_R1>(
        UDE_KIND_CT0_R1,
        0,
        N,
        r1is,
    );

    let mut r1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        r1i_at_gamma[i] = r1is[i].eval(gamma);
    }

    (r1_commit, r1i_at_gamma)
}
