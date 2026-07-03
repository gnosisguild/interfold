// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::MAX_MSG_NON_ZERO_COEFFS;
use lib::configs::default::Threshold_key::{
    L, N, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_CT, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_D,
    T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_D_NATIVE, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_E_SM,
    T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_R1, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_R2,
    T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_SK, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_CONFIGS,
};
use lib::core::Threshold_key::share_decryption::ShareDecryption;
use lib::math::polynomial::Polynomial;

fn main(
    expected_sk_commitment: pub Field,
    expected_e_sm_commitment: pub Field,
    ct_commitment: pub Field,
    ct0: [Polynomial<N>; L],
    ct1: [Polynomial<N>; L],
    sk: [Polynomial<N>; L],
    e_sm: [Polynomial<N>; L],
    r1: [Polynomial<(2 * N) - 1>; L],
    r2: [Polynomial<N - 1>; L],
    d: [Polynomial<N>; L],
    d_native_trunc: [Polynomial<MAX_MSG_NON_ZERO_COEFFS>; L],
) -> pub Field {
    let share_decryption: ShareDecryption<N, MAX_MSG_NON_ZERO_COEFFS, L, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_CT, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_SK, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_E_SM, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_R1, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_R2, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_D, T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_D_NATIVE> = ShareDecryption::new(
        T(A)RES(A)OLD_S(A)ARE_DECRYPTION_CONFIGS,
        expected_sk_commitment,
        expected_e_sm_commitment,
        ct_commitment,
        ct0,
        ct1,
        sk,
        e_sm,
        r1,
        r2,
        d,
        d_native_trunc,
    );
    share_decryption.execute()
}
