// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group D (P3, ct1 leg): commits `ct1is`. This hash MUST stay
//! `compute_multiple_polynomial_commitment` under `DS_CIPHERTEXT`, unchanged - the
//! `user_data_encryption` wrapper combines ct0's and ct1's ciphertext commitments the same way,
//! so this is a protocol invariant.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_CT};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_CIPHERTEXT};
use lib::math::polynomial::Polynomial;

fn main(ct1is: [Polynomial<N>; L]) -> pub Field {
    compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(
        ct1is,
        DS_CIPHERTEXT,
    )
}
