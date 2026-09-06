// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group A (P3): re-supplies `e0`/`e0is`/`e0_quotients`/`r2is`, redoes the CRT
//! consistency check (cheap - this is what keeps `e0is` legitimately tied to the cross-round-
//! bound `e0`, since a proof doesn't carry Round A's raw witness forward), re-derives `e0_commit`/
//! `r2_commit` for binding against Round A, and evaluates the per-limb terms the final ct0
//! identity needs.

use lib::configs::default::threshold::{
    L, N, QIS, USER_DATA_ENCRYPTION_BIT_E0, USER_DATA_ENCRYPTION_BIT_R2,
};
use lib::math::commitments::{
    compute_ude_chunk_commitment, compute_ude_multi_chunk_commitment, UDE_KIND_CT0_E0,
    UDE_KIND_CT0_R2,
};
use lib::math::polynomial::Polynomial;

fn main(
    e0: Polynomial<N>,
    e0is: [Polynomial<N>; L],
    e0_quotients: [Polynomial<N>; L],
    r2is: [Polynomial<N - 1>; L],
    gamma: pub Field,
) -> pub (Field, Field, [Field; L], [Field; L]) {
    for i in 0..L {
        for j in 0..N {
            assert(
                e0.coefficients[j]
                    == e0is[i].coefficients[j] + e0_quotients[i].coefficients[j] * QIS[i],
            );
        }
    }

    let e0_commit =
        compute_ude_chunk_commitment::<N, USER_DATA_ENCRYPTION_BIT_E0>(UDE_KIND_CT0_E0, 0, N, e0);
    let r2_commit = compute_ude_multi_chunk_commitment::<N - 1, L, USER_DATA_ENCRYPTION_BIT_R2>(
        UDE_KIND_CT0_R2,
        0,
        N,
        r2is,
    );

    let mut e0is_at_gamma: [Field; L] = [0; L];
    let mut r2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        e0is_at_gamma[i] = e0is[i].eval(gamma);
        r2i_at_gamma[i] = r2is[i].eval(gamma);
    }

    (e0_commit, r2_commit, e0is_at_gamma, r2i_at_gamma)
}
