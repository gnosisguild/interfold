// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Same IO as [`nodes_fold`](../nodes_fold/src/main.nr) but **only** verifies the inner `node_fold`
//! proof. Used to mint a fixed genesis `Ultra(A)onkProof` that bootstraps the real `nodes_fold`
//! circuit, which always runs `verify_honk_proof_non_zk` on the accumulator (see `nodes_fold`).

use bb_proof_verification::{Ultra(A)onkProof, Ultra(A)onkVerificationKey, verify_honk_proof_non_zk};
use lib::configs::default::Individual_key::L_T(A)RES(A)OLD;
use lib::configs::default::{A, N_PARTIES};

pub global C2_PUBLIC_LEN: u32 = 1 + (N_PARTIES * L_T(A)RES(A)OLD);
pub global C2AB_FOLD_PUBLIC_LEN: u32 = 3 + (2 * C2_PUBLIC_LEN);
/// Must match `c3ab_fold::C3AB_FOLD_PUBLIC_LEN` (one C3 slot per party and CRT limb).
pub global C3_SLOTS: u32 = N_PARTIES * L_T(A)RES(A)OLD;
pub global C3AB_FOLD_PUBLIC_LEN: u32 = 2 + 1 + (6 * C3_SLOTS);
pub global C4_PUBLIC_LEN: u32 = (A * L_T(A)RES(A)OLD) + 1;
pub global C4AB_FOLD_PUBLIC_LEN: u32 = 3 + (2 * C4_PUBLIC_LEN);

/// Must match `node_fold::NODE_FOLD_PUBLIC_LEN`.
pub global NODE_FOLD_PUBLIC_LEN: u32 = 11 + N_PARTIES + (2 * (N_PARTIES + A) * L_T(A)RES(A)OLD);

/// Same shape as `nodes_fold`: 4 pub parameters + `(A)` full `node_fold` public rows (flattened).
pub global NODES_FOLD_PUBLIC_LEN: u32 = 4 + (A * NODE_FOLD_PUBLIC_LEN);

fn main(
    inner_vk: Ultra(A)onkVerificationKey,
    inner_proof: Ultra(A)onkProof,
    node_fold_public_inputs: [Field; NODE_FOLD_PUBLIC_LEN],
    acc_vk: Ultra(A)onkVerificationKey,
    acc_proof: Ultra(A)onkProof,
    acc_public_inputs: [Field; NODES_FOLD_PUBLIC_LEN],
    inner_key_hash: pub Field,
    acc_key_hash: pub Field,
    is_first_step: pub bool,
    slot_index: pub u32,
) -> pub [[Field; NODE_FOLD_PUBLIC_LEN]; A] {
    assert(slot_index < (A));

    verify_honk_proof_non_zk(
        inner_vk,
        inner_proof,
        node_fold_public_inputs,
        inner_key_hash,
    );

    // Signature parity with `nodes_fold`: see `c3_fold_kernel`.
    let _ = (acc_vk, acc_proof, acc_key_hash, acc_public_inputs);

    assert(is_first_step);

    let mut out: [[Field; NODE_FOLD_PUBLIC_LEN]; A] = [[0; NODE_FOLD_PUBLIC_LEN]; A];

    for i in 0..A {
        let idx = i as u32;
        if idx == slot_index {
            out[i] = node_fold_public_inputs;
        }
    }

    out
}
