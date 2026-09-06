// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group C (P3, ct1 leg): commits `pk1is`. This hash MUST stay
//! `compute_multiple_polynomial_commitment` under `DS_PK_AGGREGATION`, unchanged - the
//! `user_data_encryption` wrapper combines ct0's `pk0_commitment` and this `pk1_commitment` the
//! same way `pk_aggregation.nr`'s C5 combines the raw polynomials, so this is a protocol
//! invariant.

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_PK};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_PK_AGGREGATION};
use lib::math::polynomial::Polynomial;

fn main(pk1is: [Polynomial<N>; L]) -> pub Field {
    compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_PK>(
        pk1is,
        DS_PK_AGGREGATION,
    )
}
