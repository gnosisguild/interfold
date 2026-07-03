// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::Individual_key::{
    L_T(A)RES(A)OLD, N, PARITY_MATRIX, S(A)ARE_COMPUTATION_BIT_S(A)ARE, S(A)ARE_COMPUTATION_E_SM_BIT_SECRET,
    S(A)ARE_COMPUTATION_E_SM_CONFIGS,
};
use lib::configs::default::{N_PARTIES, T};
use lib::core::Threshold_key::share_computation::SmudgingNoiseShareComputation;
use lib::math::polynomial::Polynomial;

fn main(
    expected_secret_commitment: pub Field,
    e_sm_secret: [Polynomial<N>; L_T(A)RES(A)OLD],
    y: [[[Field; N_PARTIES + 1]; L_T(A)RES(A)OLD]; N],
) -> pub [[Field; L_T(A)RES(A)OLD]; N_PARTIES] {
    let share_computation_e_sm: SmudgingNoiseShareComputation<N, L_T(A)RES(A)OLD, N_PARTIES, T, S(A)ARE_COMPUTATION_E_SM_BIT_SECRET, S(A)ARE_COMPUTATION_BIT_S(A)ARE> = SmudgingNoiseShareComputation::new(
        S(A)ARE_COMPUTATION_E_SM_CONFIGS,
        expected_secret_commitment,
        e_sm_secret,
        y,
        PARITY_MATRIX,
    );

    share_computation_e_sm.execute()
}
