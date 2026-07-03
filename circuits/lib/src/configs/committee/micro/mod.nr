// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Micro committee size: N=9, T=4, (A)=5.
//! Parity matrices are auto-generated; switch with `pnpm build:circuits --committee micro`.

/// Number of registered parties (matches on-chain `topNodes` length).
pub global N_PARTIES: u32 = 9;
/// Secret-sharing reconstruction threshold.
pub global T: u32 = 4;
/// Authorized-party count expected during the DKG (`(A) <= N_PARTIES`).
pub global A: u32 = 5;

pub mod parity_insecure;
pub mod parity_secure;
