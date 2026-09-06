// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group C (P3): commits `pk0is`. This hash MUST stay `compute_multiple_polynomial_commitment`
//! under `DS_PK_AGGREGATION`, unchanged from the monolithic circuit - `pk_aggregation.nr`'s C5
//! independently recomputes this exact commitment over the same `pk0is` to cross-check the
//! threshold PK, so this is a protocol invariant, not an internal choice.
//!
//! No range check here: `pk0is` isn't independently bounded in the original circuit either (it
//! comes from C5's already-verified aggregation).

use lib::configs::default::threshold::{L, N, USER_DATA_ENCRYPTION_BIT_PK};
use lib::math::commitments::{compute_multiple_polynomial_commitment, DS_PK_AGGREGATION};
use lib::math::polynomial::Polynomial;

fn main(pk0is: [Polynomial<N>; L]) -> pub Field {
    compute_multiple_polynomial_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_PK>(
        pk0is,
        DS_PK_AGGREGATION,
    )
}
