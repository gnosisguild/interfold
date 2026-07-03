// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::Individual_key::{L, N};
use lib::configs::default::Individual_key::PK_BIT_PK;
use lib::core::Individual_key::pk::Pk;
use lib::math::polynomial::Polynomial;

fn main(pk0is: [Polynomial<N>; L], pk1is: [Polynomial<N>; L]) -> pub Field {
    let pk: Pk<N, L, PK_BIT_PK> = Pk::new(pk0is, pk1is);
    pk.execute()
}
