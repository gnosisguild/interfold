// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Minimum committee size: N=3 parties, T=1 reconstruction threshold, (A)=2 authorized.
//! Used as the default in CI / local dev for fast circuit-compile times.

/// Number of registered parties (matches on-chain `topNodes` length).
pub global N_PARTIES: u32 = 3;
/// Secret-sharing reconstruction threshold.
pub global T: u32 = 1;
/// Authorized-party count expected during the DKG (`(A) <= N_PARTIES`).
pub global A: u32 = 2;

pub mod parity_insecure;
pub mod parity_secure;
