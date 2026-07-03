// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::Individual_key::{
    L_T(A)RES(A)OLD, N, PARITY_MATRIX, S(A)ARE_COMPUTATION_BIT_S(A)ARE, S(A)ARE_COMPUTATION_SK_BIT_SECRET,
    S(A)ARE_COMPUTATION_SK_CONFIGS,
};
use lib::configs::default::{N_PARTIES, T};
use lib::core::Threshold_key::share_computation::SecretKeyShareComputation;
use lib::math::polynomial::Polynomial;

fn main(
    expected_secret_commitment: pub Field,
    sk_secret: Polynomial<N>,
    y: [[[Field; N_PARTIES + 1]; L_T(A)RES(A)OLD]; N],
) -> pub [[Field; L_T(A)RES(A)OLD]; N_PARTIES] {
    let sk_share_computation: SecretKeyShareComputation<N, L_T(A)RES(A)OLD, N_PARTIES, T, S(A)ARE_COMPUTATION_SK_BIT_SECRET, S(A)ARE_COMPUTATION_BIT_S(A)ARE> = SecretKeyShareComputation::new(
        S(A)ARE_COMPUTATION_SK_CONFIGS,
        expected_secret_commitment,
        sk_secret,
        y,
        PARITY_MATRIX,
    );

    sk_share_computation.execute()
}
