// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group D (P3): commits `ct0is`. This hash MUST stay `compute_multiple_polynomial_commitment`
//! under `DS_CIPHERTEXT`, unchanged from the monolithic circuit - the `user_data_encryption`
//! wrapper recomputes the combined ciphertext commitment from ct0's and ct1's commitment values,
//! so this exact formula is a protocol invariant, not an internal choice.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_CT};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_CIPHERTEXT};
use lib::math::polynomial::Polynomial;

fn main(ct0is: [Polynomial<N>; L]) -> pub Field {
    compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(
        ct0is,
        DS_CIPHERTEXT,
    )
}
