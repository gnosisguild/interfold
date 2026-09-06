// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group A (P3, ct1 leg): range-checks `e1`/`p2is` and commits both plus `u`. `u` isn't
//! range-checked here - it's the same value ct0 already range-checked, re-witnessed only to prove
//! ct1 used the identical `u` (the `user_data_encryption` wrapper compares the two independently
//! computed `u_commitment`s for equality). `u_commitment` MUST stay
//! `compute_single_polynomial_commitment` under `DS_USER_DATA_ENCRYPTION_COMMITMENT`, unchanged,
//! for that cross-leg check to work.

use lib::configs::default::threshold::{
    L, N, USER_DATA_ENCRYPTION_BIT_E1, USER_DATA_ENCRYPTION_BIT_P2, USER_DATA_ENCRYPTION_BIT_U,
    USER_DATA_ENCRYPTION_E1_BOUND, USER_DATA_ENCRYPTION_P2_BOUNDS,
};
use lib::math::commitments::{
    compute_single_polynomial_commitment, compute_ude_chunk_commitment,
    compute_ude_multi_chunk_commitment, DS_USER_DATA_ENCRYPTION_COMMITMENT, UDE_KIND_CT1_E1,
    UDE_KIND_CT1_P2,
};
use lib::math::polynomial::Polynomial;

fn main(
    e1: Polynomial<N>,
    p2is: [Polynomial<N - 1>; L],
    u: Polynomial<N>,
) -> pub (Field, Field, Field) {
    e1.range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_E1>(
        USER_DATA_ENCRYPTION_E1_BOUND,
        USER_DATA_ENCRYPTION_E1_BOUND,
    );
    for i in 0..L {
        p2is[i].range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_P2>(
            USER_DATA_ENCRYPTION_P2_BOUNDS[i],
            USER_DATA_ENCRYPTION_P2_BOUNDS[i],
        );
    }

    let e1_commit =
        compute_ude_chunk_commitment::<N, USER_DATA_ENCRYPTION_BIT_E1>(UDE_KIND_CT1_E1, 0, N, e1);
    let p2_commit = compute_ude_multi_chunk_commitment::<N - 1, L, USER_DATA_ENCRYPTION_BIT_P2>(
        UDE_KIND_CT1_P2,
        0,
        N,
        p2is,
    );
    let u_commitment = compute_single_polynomial_commitment::<N, USER_DATA_ENCRYPTION_BIT_U>(
        u,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    );

    (e1_commit, p2_commit, u_commitment)
}
