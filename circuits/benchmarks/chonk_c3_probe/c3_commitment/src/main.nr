// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use lib::configs::default::dkg::{
    N, SHARE_COMPUTATION_CHUNK_SIZE, SHARE_COMPUTATION_N_CHUNKS, SHARE_ENCRYPTION_BIT_MSG,
};
use lib::math::commitments::compute_sc_party_share_root_from_polynomial;
use lib::math::polynomial::Polynomial;

/// Recompute the C2 share-root commitment for a selected C3 slot. This is a
/// probe helper so one generated real witness can be assigned to several
/// slot-shaped leaves without changing the production Rust helpers.
fn main(party_idx: u32, mod_idx: u32, message: Polynomial<N>) -> pub Field {
    compute_sc_party_share_root_from_polynomial::<N, SHARE_COMPUTATION_N_CHUNKS, SHARE_COMPUTATION_CHUNK_SIZE, SHARE_ENCRYPTION_BIT_MSG>(
        party_idx,
        mod_idx,
        message,
    )
}
