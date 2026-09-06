// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A, group B (P3): range-checks `r1is`. Split out on its own because it's the single
//! largest witness group in ct0 (measured ~1.22M gates at N=16384, L=5) - bundling it with
//! anything else would exceed the browser gate budget.

use lib::configs::default::threshold::{
    L, N, USER_DATA_ENCRYPTION_BIT_R1, USER_DATA_ENCRYPTION_R1_LOW_BOUNDS,
    USER_DATA_ENCRYPTION_R1_UP_BOUNDS,
};
use lib::math::commitments::{compute_ude_multi_chunk_commitment, UDE_KIND_CT0_R1};
use lib::math::polynomial::Polynomial;

fn main(r1is: [Polynomial<(2 * N) - 1>; L]) -> pub Field {
    for i in 0..L {
        r1is[i].range_check_2bounds::<USER_DATA_ENCRYPTION_BIT_R1>(
            USER_DATA_ENCRYPTION_R1_UP_BOUNDS[i],
            USER_DATA_ENCRYPTION_R1_LOW_BOUNDS[i],
        );
    }

    compute_ude_multi_chunk_commitment::<(2 * N) - 1, L, USER_DATA_ENCRYPTION_BIT_R1>(
        UDE_KIND_CT0_R1,
        0,
        N,
        r1is,
    )
}
