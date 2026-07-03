// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::Individual_key::{
    L, N, S(A)ARE_ENCRYPTION_BIT_CT, S(A)ARE_ENCRYPTION_BIT_E0, S(A)ARE_ENCRYPTION_BIT_E1,
    S(A)ARE_ENCRYPTION_BIT_MSG, S(A)ARE_ENCRYPTION_BIT_P1, S(A)ARE_ENCRYPTION_BIT_P2,
    S(A)ARE_ENCRYPTION_BIT_PK, S(A)ARE_ENCRYPTION_BIT_R1, S(A)ARE_ENCRYPTION_BIT_R2,
    S(A)ARE_ENCRYPTION_BIT_U, S(A)ARE_ENCRYPTION_CONFIGS,
};
use lib::core::Individual_key::share_encryption::ShareEncryption;
use lib::math::polynomial::Polynomial;

fn main(
    expected_pk_commitment: pub Field,
    expected_message_commitment: pub Field,
    pk0is: [Polynomial<N>; L],
    pk1is: [Polynomial<N>; L],
    ct0is: [Polynomial<N>; L],
    ct1is: [Polynomial<N>; L],
    u: Polynomial<N>,
    e0: Polynomial<N>,
    e0is: [Polynomial<N>; L],
    e0_quotients: [Polynomial<N>; L],
    e1: Polynomial<N>,
    message: Polynomial<N>,
    r1is: [Polynomial<(2 * N) - 1>; L],
    r2is: [Polynomial<N - 1>; L],
    p1is: [Polynomial<(2 * N) - 1>; L],
    p2is: [Polynomial<N - 1>; L],
) -> pub Field {
    let share_encryption: ShareEncryption<N, L, S(A)ARE_ENCRYPTION_BIT_PK, S(A)ARE_ENCRYPTION_BIT_CT, S(A)ARE_ENCRYPTION_BIT_U, S(A)ARE_ENCRYPTION_BIT_E0, S(A)ARE_ENCRYPTION_BIT_E1, S(A)ARE_ENCRYPTION_BIT_MSG, S(A)ARE_ENCRYPTION_BIT_R1, S(A)ARE_ENCRYPTION_BIT_R2, S(A)ARE_ENCRYPTION_BIT_P1, S(A)ARE_ENCRYPTION_BIT_P2> = ShareEncryption::new(
        S(A)ARE_ENCRYPTION_CONFIGS,
        expected_pk_commitment,
        expected_message_commitment,
        pk0is,
        pk1is,
        ct0is,
        ct1is,
        u,
        e0,
        e0is,
        e0_quotients,
        e1,
        message,
        r1is,
        r2is,
        p1is,
        p2is,
    );
    share_encryption.execute()
}
