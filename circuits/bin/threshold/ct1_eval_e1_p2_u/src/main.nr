// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group A (P3, ct1 leg): re-supplies `e1`/`p2is`/`u`, re-derives their commitments for
//! binding against Round A, and evaluates them at gamma.

use lib::configs::default::threshold::{
    L, N, USER_DATA_ENCRYPTION_BIT_E1, USER_DATA_ENCRYPTION_BIT_P2, USER_DATA_ENCRYPTION_BIT_U,
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
    gamma: pub Field,
) -> pub (Field, Field, Field, Field, [Field; L], Field) {
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

    let e1_at_gamma = e1.eval(gamma);
    let mut p2i_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        p2i_at_gamma[i] = p2is[i].eval(gamma);
    }
    let u_at_gamma = u.eval(gamma);

    (e1_commit, p2_commit, u_commitment, e1_at_gamma, p2i_at_gamma, u_at_gamma)
}
