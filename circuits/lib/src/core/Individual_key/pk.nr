// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::math::commitments::compute_dkg_pk_commitment;
use crate::math::polynomial::Polynomial;

/// DKG public key commitment (C0).
///
/// **Role:** Bind the ciphernode DKG public key used to encrypt outgoing shares in DKG.
///
/// **Produces:**
/// - `commit(pk_dkg)` -> C3 (share encryption).
pub struct Pk<let N: u32, let L: u32, let BIT_PK: u32> {
    /// DKG public key first component for each CRT modulus.
    /// pk0[i] is a degree N-1 polynomial for modulus q_i.
    pk0: [Polynomial<N>; L],
    /// DKG public key second component for each CRT modulus.
    /// pk1[i] is a degree N-1 polynomial for modulus q_i.
    pk1: [Polynomial<N>; L],
}

impl<let N: u32, let L: u32, let BIT_PK: u32> Pk<N, L, BIT_PK> {
    /// Creates a new DKG public key commitment circuit instance.
    ///
    /// # Arguments
    ///
    /// * `pk0` - First component of DKG public key (one polynomial per CRT modulus)
    /// * `pk1` - Second component of DKG public key (one polynomial per CRT modulus)
    pub fn new(pk0: [Polynomial<N>; L], pk1: [Polynomial<N>; L]) -> Self {
        Pk { pk0, pk1 }
    }

    /// Executes the commitment circuit and returns the binding commitment.
    pub fn execute(self) -> Field {
        compute_dkg_pk_commitment::<N, L, BIT_PK>(self.pk0, self.pk1)
    }
}
