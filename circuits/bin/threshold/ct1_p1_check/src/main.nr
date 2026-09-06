// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group B (P3, ct1 leg): range-checks `p1is`. Split out on its own for the same reason
//! as ct0's `r1is` - it's the single largest witness group in ct1.

use lib::configs::default::threshold::{
    L, N, USER_DATA_ENCRYPTION_BIT_P1, USER_DATA_ENCRYPTION_P1_BOUNDS,
};
use lib::math::commitments::{compute_ude_multi_chunk_commitment, UDE_KIND_CT1_P1};
use lib::math::polynomial::Polynomial;

fn main(p1is: [Polynomial<(2 * N) - 1>; L]) -> pub Field {
    for i in 0..L {
        p1is[i].range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_P1>(
            USER_DATA_ENCRYPTION_P1_BOUNDS[i],
            USER_DATA_ENCRYPTION_P1_BOUNDS[i],
        );
    }

    compute_ude_multi_chunk_commitment::<(2 * N) - 1, L, USER_DATA_ENCRYPTION_BIT_P1>(
        UDE_KIND_CT1_P1,
        0,
        N,
        p1is,
    )
}
