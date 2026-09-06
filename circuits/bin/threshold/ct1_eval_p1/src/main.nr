// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group B (P3, ct1 leg): re-supplies `p1is`, re-derives `p1_commit` for binding against
//! Round A, and evaluates it at gamma.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_P1};
use lib::math::commitments::{compute_ude_multi_chunk_commitment, UDE_KIND_CT1_P1};
use lib::math::polynomial::Polynomial;

fn main(p1is: [Polynomial<(2 * N) - 1>; L], gamma: pub Field) -> pub (Field, [Field; L]) {
    let p1_commit = compute_ude_multi_chunk_commitment::<(2 * N) - 1, L, USER_DATA_ENCRYPTION_BIT_P1>(
        UDE_KIND_CT1_P1,
        0,
        N,
        p1is,
    );

    let mut p1i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        p1i_at_gamma[i] = p1is[i].eval(gamma);
    }

    (p1_commit, p1i_at_gamma)
}
