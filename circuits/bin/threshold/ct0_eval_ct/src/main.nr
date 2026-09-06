// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group D (P3): re-supplies `ct0is`, re-derives `ct0_commitment` (same protocol-
//! invariant formula as `ct0_ct_commit`, for binding against Round A) and evaluates the per-limb
//! LHS terms of the final ct0 identity.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_CT};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_CIPHERTEXT};
use lib::math::polynomial::Polynomial;

fn main(ct0is: [Polynomial<N>; L], gamma: pub Field) -> pub (Field, [Field; L]) {
    let ct0_commitment = compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(
        ct0is,
        DS_CIPHERTEXT,
    );

    let mut ct0is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        ct0is_at_gamma[i] = ct0is[i].eval(gamma);
    }

    (ct0_commitment, ct0is_at_gamma)
}
