// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group E (P3): re-supplies `u`/`k1`, re-derives their commitments (same protocol-
//! invariant formula as `ct0_u_k1_check`, for binding against Round A) and evaluates them at
//! gamma.

use lib::configs::default::threshold::{N, USER_DATA_ENCRYPTION_BIT_K, USER_DATA_ENCRYPTION_BIT_U};
use lib::math::commitments::{
    compute_single_polynomial_commitment, DS_USER_DATA_ENCRYPTION_COMMITMENT,
};
use lib::math::polynomial::Polynomial;

fn main(u: Polynomial<N>, k1: Polynomial<N>, gamma: pub Field) -> pub (Field, Field, Field, Field) {
    let u_commitment = compute_single_polynomial_commitment::<N, USER_DATA_ENCRYPTION_BIT_U>(
        u,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    );
    let k1_commitment = compute_single_polynomial_commitment::<N, USER_DATA_ENCRYPTION_BIT_K>(
        k1,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    );

    let u_at_gamma = u.eval(gamma);
    let k1_at_gamma = k1.eval(gamma);

    (u_commitment, k1_commitment, u_at_gamma, k1_at_gamma)
}
