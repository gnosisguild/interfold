// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::Threshold_key::{
    L, N, USER_DATA_ENCRYPTION_BIT_CT, USER_DATA_ENCRYPTION_BIT_E1, USER_DATA_ENCRYPTION_BIT_P1,
    USER_DATA_ENCRYPTION_BIT_P2, USER_DATA_ENCRYPTION_BIT_PK, USER_DATA_ENCRYPTION_BIT_U,
    USER_DATA_ENCRYPTION_CT1_CONFIGS,
};
use lib::core::Threshold_key::user_data_encryption_ct1::UserDataEncryptionCt1;
use lib::math::polynomial::Polynomial;

fn main(
    pk1is: [Polynomial<N>; L],
    ct1is: [Polynomial<N>; L],
    u: Polynomial<N>,
    e1: Polynomial<N>,
    p1is: [Polynomial<(2 * N) - 1>; L],
    p2is: [Polynomial<N - 1>; L],
) -> pub (Field, Field, Field) {
    let circuit: UserDataEncryptionCt1<N, L, USER_DATA_ENCRYPTION_BIT_PK, USER_DATA_ENCRYPTION_BIT_CT, USER_DATA_ENCRYPTION_BIT_U, USER_DATA_ENCRYPTION_BIT_E1, USER_DATA_ENCRYPTION_BIT_P1, USER_DATA_ENCRYPTION_BIT_P2> = UserDataEncryptionCt1::new(
        USER_DATA_ENCRYPTION_CT1_CONFIGS,
        pk1is,
        ct1is,
        u,
        e1,
        p1is,
        p2is,
    );

    circuit.execute()
}
