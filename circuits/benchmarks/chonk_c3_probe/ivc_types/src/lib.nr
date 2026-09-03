/// Field lengths pinned by bb.js 5.1.0 and the v5.1.0 Chonk ABI.
pub global MEGA_VK_LENGTH_IN_FIELDS: u32 = 163;
pub global CHONK_VK_LENGTH_IN_FIELDS: u32 = 115;
pub global CHONK_PROOF_LENGTH: u32 = 1270;
pub global ROLLUP_HONK_PROOF_LENGTH: u32 = 480;

pub global PROOF_TYPE_OINK: u32 = 1;
pub global PROOF_TYPE_HN_TAIL: u32 = 8;
pub global PROOF_TYPE_HN_FINAL: u32 = 7;
pub global PROOF_TYPE_CHONK: u32 = 9;

use lib::configs::default::dkg::L_THRESHOLD;
use lib::configs::default::N_PARTIES;

/// One C3 slot per registered party and threshold modulus. The active Noir config selects the
/// committee and preset, keeping the probe ABI aligned with the production c3_fold circuit.
pub global C3_SLOTS: u32 = N_PARTIES * L_THRESHOLD;
/// The ECCVM fixed bound in bb.js 5.1.0 requires the non-local slots to be split into two Chonk
/// proofs. The production c3ab wrapper verifies both tubes and merges their sparse accumulators.
pub global C3_BATCH_SIZE: u32 = (C3_SLOTS - L_THRESHOLD) / 2;
pub global C3_ACCUMULATOR_LEN: u32 = 3 * C3_SLOTS;
pub global C3_FOLD_PREFIX_LEN: u32 = 6;
pub global C3_FOLD_PUBLIC_LEN: u32 = C3_FOLD_PREFIX_LEN + C3_ACCUMULATOR_LEN;

pub struct VerificationKey {
    pub key: [Field; MEGA_VK_LENGTH_IN_FIELDS],
    pub hash: Field,
}
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
