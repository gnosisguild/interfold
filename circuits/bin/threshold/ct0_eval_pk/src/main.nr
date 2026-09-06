// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group C (P3): re-supplies `pk0is`, re-derives `pk0_commitment` (same protocol-
//! invariant formula as `ct0_pk_commit`, for binding against Round A) and evaluates the per-limb
//! terms the final ct0 identity needs.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_PK};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_PK_AGGREGATION};
use lib::math::polynomial::Polynomial;

fn main(pk0is: [Polynomial<N>; L], gamma: pub Field) -> pub (Field, [Field; L]) {
    let pk0_commitment = compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_PK>(
        pk0is,
        DS_PK_AGGREGATION,
    );

    let mut pk0is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        pk0is_at_gamma[i] = pk0is[i].eval(gamma);
    }

    (pk0_commitment, pk0is_at_gamma)
}
