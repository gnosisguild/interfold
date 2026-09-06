// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group C (P3, ct1 leg): re-supplies `pk1is`, re-derives `pk1_commitment` (same
//! protocol-invariant formula as `ct1_pk_commit`) and evaluates it at gamma.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_PK};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_PK_AGGREGATION};
use lib::math::polynomial::Polynomial;

fn main(pk1is: [Polynomial<N>; L], gamma: pub Field) -> pub (Field, [Field; L]) {
    let pk1_commitment = compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_PK>(
        pk1is,
        DS_PK_AGGREGATION,
    );

    let mut pk1is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        pk1is_at_gamma[i] = pk1is[i].eval(gamma);
    }

    (pk1_commitment, pk1is_at_gamma)
}
