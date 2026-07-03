// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::A;
use lib::configs::default::Threshold_key::{CRP, L, N, PK_AGGREGATION_BIT_PK, PK_AGGREGATION_CONFIGS};
use lib::core::Threshold_key::pk_aggregation::PkAggregation;
use lib::math::polynomial::Polynomial;

fn main(
    expected_threshold_pk_commitments: pub [Field; A],
    pk0: [[Polynomial<N>; L]; A],
    pk0_agg: [Polynomial<N>; L],
) -> pub Field {
    let pk_aggregation: PkAggregation<N, A, L, PK_AGGREGATION_BIT_PK> = PkAggregation::new(
        PK_AGGREGATION_CONFIGS,
        CRP,
        expected_threshold_pk_commitments,
        pk0,
        pk0_agg,
    );

    pk_aggregation.execute()
}
