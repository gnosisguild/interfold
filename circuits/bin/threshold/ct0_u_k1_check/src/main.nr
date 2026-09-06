// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group E (P3): range-checks `u`/`k1` and commits both. Cheap on its own (neither
//! scales with L), so it's the one group that doesn't need splitting for scale reasons - it's
//! split out here only because it doesn't belong in any of the other four groups.
//!
//! `u_commitment`/`k1_commitment` MUST stay `compute_single_polynomial_commitment` under
//! `DS_USER_DATA_ENCRYPTION_COMMITMENT`, unchanged from the monolithic circuit - the
//! `user_data_encryption` wrapper compares ct0's and ct1's `u_commitment` directly for equality,
//! so both legs must keep computing it the same way.

use lib::configs::default::threshold::{
    N, USER_DATA_ENCRYPTION_BIT_K, USER_DATA_ENCRYPTION_BIT_U, USER_DATA_ENCRYPTION_K1_LOW_BOUND,
    USER_DATA_ENCRYPTION_K1_UP_BOUND, USER_DATA_ENCRYPTION_U_BOUND,
};
use lib::math::commitments::{
    compute_single_polynomial_commitment, DS_USER_DATA_ENCRYPTION_COMMITMENT,
};
use lib::math::polynomial::Polynomial;

fn main(u: Polynomial<N>, k1: Polynomial<N>) -> pub (Field, Field) {
    u.range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_U>(
        USER_DATA_ENCRYPTION_U_BOUND,
        USER_DATA_ENCRYPTION_U_BOUND,
    );
    k1.range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_K>(
        USER_DATA_ENCRYPTION_K1_UP_BOUND,
        USER_DATA_ENCRYPTION_K1_LOW_BOUND,
    );

    let u_commitment = compute_single_polynomial_commitment::<N, USER_DATA_ENCRYPTION_BIT_U>(
        u,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    );
    let k1_commitment = compute_single_polynomial_commitment::<N, USER_DATA_ENCRYPTION_BIT_K>(
        k1,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    );

    (u_commitment, k1_commitment)
}
