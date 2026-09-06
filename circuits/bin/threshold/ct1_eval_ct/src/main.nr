// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round B, group D (P3, ct1 leg): re-supplies `ct1is`, re-derives `ct1_commitment` (same
//! protocol-invariant formula as `ct1_ct_commit`) and evaluates the per-limb LHS terms of the
//! final ct1 identity.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_CT};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_CIPHERTEXT};
use lib::math::polynomial::Polynomial;

fn main(ct1is: [Polynomial<N>; L], gamma: pub Field) -> pub (Field, [Field; L]) {
    let ct1_commitment = compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(
        ct1is,
        DS_CIPHERTEXT,
    );

    let mut ct1is_at_gamma: [Field; L] = [0; L];
    for i in 0..L {
        ct1is_at_gamma[i] = ct1is[i].eval(gamma);
    }

    (ct1_commitment, ct1is_at_gamma)
}
