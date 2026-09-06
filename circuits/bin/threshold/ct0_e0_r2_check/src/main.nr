// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group A (P3): range-checks `e0`, range-checks `r2is`, and checks `e0`'s CRT
//! consistency against `e0is`/`e0_quotients`. Split out of the monolithic
//! `user_data_encryption_ct0` because at N=16384, L=5 the full circuit no longer fits a single
//! browser-side proof; this group alone comfortably does (measured ~1.52M gates).
//!
//! `e0is`/`e0_quotients` aren't independently committed here - Round B (`ct0_eval_e0_r2`)
//! re-supplies them and re-checks this same relation against the cross-round-bound `e0`, rather
//! than needing a separate commitment scheme for them.

use lib::configs::default::threshold::{
    L, N, QIS, USER_DATA_ENCRYPTION_BIT_E0, USER_DATA_ENCRYPTION_BIT_R2,
    USER_DATA_ENCRYPTION_E0_BOUND, USER_DATA_ENCRYPTION_R2_BOUNDS,
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
) -> pub (Field, Field) {
    e0.range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_E0>(
        USER_DATA_ENCRYPTION_E0_BOUND,
        USER_DATA_ENCRYPTION_E0_BOUND,
    );
    for i in 0..L {
        r2is[i].range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_R2>(
            USER_DATA_ENCRYPTION_R2_BOUNDS[i],
            USER_DATA_ENCRYPTION_R2_BOUNDS[i],
        );
    }

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

    (e0_commit, r2_commit)
}
