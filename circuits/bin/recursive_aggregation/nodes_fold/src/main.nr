// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Sequential fold of `(A)` per-node `node_fold` proofs (non-ZK), one authorized slot per step, chained with
//! the previous `nodes_fold` non-ZK proof, or a fixed genesis proof from `nodes_fold_kernel` on the
//! first step (see `crates/zk-prover` and `nodes_fold_kernel`).

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

/// Same shape as `c6_fold`: 4 pub parameters + `(A)` full `node_fold` public rows (flattened).
pub global NODES_FOLD_PUBLIC_LEN: u32 = 4 + (A * NODE_FOLD_PUBLIC_LEN);

pub global NODES_FOLD_PARAM_PUBLIC_LEN: u32 = NODES_FOLD_PUBLIC_LEN - (A * NODE_FOLD_PUBLIC_LEN);

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

    verify_honk_proof_non_zk(acc_vk, acc_proof, acc_public_inputs, acc_key_hash);

    let mut out: [[Field; NODE_FOLD_PUBLIC_LEN]; A] = [[0; NODE_FOLD_PUBLIC_LEN]; A];

    for i in 0..A {
        let idx = i as u32;
        if is_first_step {
            if idx == slot_index {
                out[i] = node_fold_public_inputs;
            }
        } else if idx == slot_index {
            for k in 0..NODE_FOLD_PUBLIC_LEN {
                assert(
                    acc_public_inputs[NODES_FOLD_PARAM_PUBLIC_LEN + i * NODE_FOLD_PUBLIC_LEN + k]
                        == 0,
                );
            }
            out[i] = node_fold_public_inputs;
        } else {
            for k in 0..NODE_FOLD_PUBLIC_LEN {
                out[i][k] =
                    acc_public_inputs[NODES_FOLD_PARAM_PUBLIC_LEN + i * NODE_FOLD_PUBLIC_LEN + k];
            }
        }
    }

    out
}
