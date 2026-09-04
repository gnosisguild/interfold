// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

/// Field lengths pinned by bb.js 5.1.0 and the v5.1.0 Chonk ABI.
pub global MEGA_VK_LENGTH_IN_FIELDS: u32 = 163;
pub global CHONK_VK_LENGTH_IN_FIELDS: u32 = 115;
pub global CHONK_PROOF_LENGTH: u32 = 1270;
pub global ROLLUP_HONK_PROOF_LENGTH: u32 = 480;

pub global PROOF_TYPE_OINK: u32 = 1;
pub global PROOF_TYPE_HN: u32 = 2;
pub global PROOF_TYPE_HN_TAIL: u32 = 8;
pub global PROOF_TYPE_HN_FINAL: u32 = 7;
pub global PROOF_TYPE_CHONK: u32 = 9;

use lib::configs::default::dkg::L_THRESHOLD;
use lib::configs::default::N_PARTIES;

/// One C3 slot per registered party and threshold modulus. The active Noir config selects the
/// committee and preset, keeping the probe ABI aligned with the production c3_fold circuit.
pub global C3_SLOTS: u32 = N_PARTIES * L_THRESHOLD;
pub global C3_LEAF_COUNT: u32 = C3_SLOTS - L_THRESHOLD;
/// The ECCVM fixed bound in bb.js 5.1.0 requires the non-local slots to be split into two Chonk
/// proofs. The production c3ab wrapper verifies both tubes and merges their sparse accumulators.
pub global C3_BATCH_SIZE: u32 = (C3_SLOTS - L_THRESHOLD) / 2;
/// The Small-committee leaf stack is capped below the 18-leaf batch by the Chonk append-offset
/// bound. Nine leaves keeps the stack within that bound while allowing the resulting four tubes
/// to be closed as two binary root folds and one ordinary merge.
pub global C3_LEAF_STACK_SIZE: u32 = 9;
pub global C3_ACCUMULATOR_LEN: u32 = 3 * C3_SLOTS;
pub global C3_FOLD_PREFIX_LEN: u32 = 6;
pub global C3_FOLD_PUBLIC_LEN: u32 = C3_FOLD_PREFIX_LEN + C3_ACCUMULATOR_LEN;

pub struct VerificationKey {
    pub key: [Field; MEGA_VK_LENGTH_IN_FIELDS],
    pub hash: Field,
}

/// Return data emitted by one C3 leaf app and consumed by its kernel.
pub struct C3LeafOutput {
    pub slot: u32,
    pub pk: Field,
    pub msg: Field,
    pub ct: Field,
}

/// Kernel state carried between C3 leaf app/kernel pairs.
///
/// The count and slot list make the sequential databus transition reject
/// duplicate slots. The columns use a compact append-order layout; the final
/// hiding kernel expands them back to the global sparse accumulator.
pub struct C3AccumulatorState {
    pub slots: [u32; C3_LEAF_STACK_SIZE],
    pub count: u32,
    pub pk: [Field; C3_LEAF_STACK_SIZE],
    pub msg: [Field; C3_LEAF_STACK_SIZE],
    pub ct: [Field; C3_LEAF_STACK_SIZE],
}

pub fn empty_c3_state() -> C3AccumulatorState {
    C3AccumulatorState {
        slots: [C3_SLOTS; C3_LEAF_STACK_SIZE],
        count: 0,
        pk: [0; C3_LEAF_STACK_SIZE],
        msg: [0; C3_LEAF_STACK_SIZE],
        ct: [0; C3_LEAF_STACK_SIZE],
    }
}

pub fn append_c3_leaf(mut state: C3AccumulatorState, leaf: C3LeafOutput) -> C3AccumulatorState {
    assert(leaf.slot < C3_SLOTS);
    assert(state.count < C3_LEAF_STACK_SIZE);
    assert(leaf.pk != 0);

    for i in 0..C3_LEAF_STACK_SIZE {
        if i < state.count {
            assert(state.slots[i] != leaf.slot);
        }
    }

    let local_slot = state.count;
    state.count += 1;
    state.slots[local_slot] = leaf.slot;
    state.pk[local_slot] = leaf.pk;
    state.msg[local_slot] = leaf.msg;
    state.ct[local_slot] = leaf.ct;
    state
}

pub fn c3_state_to_accumulator(state: C3AccumulatorState) -> [Field; C3_ACCUMULATOR_LEN] {
    assert(state.count == C3_LEAF_STACK_SIZE);

    let mut output: [Field; C3_ACCUMULATOR_LEN] = [0; C3_ACCUMULATOR_LEN];
    for i in 0..C3_SLOTS {
        for j in 0..C3_LEAF_STACK_SIZE {
            if state.slots[j] == i {
                output[i] = state.pk[j];
                output[C3_SLOTS + i] = state.msg[j];
                output[(2 * C3_SLOTS) + i] = state.ct[j];
            }
        }
    }
    output
}
