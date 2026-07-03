// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Same IO as [`c6_fold`](../c6_fold/src/main.nr) but **only** verifies the inner
//! `ThresholdShareDecryption` proof. Used to mint a fixed genesis `Ultra(A)onkProof` that bootstraps
//! the real `c6_fold` circuit, which always runs `verify_honk_proof_non_zk` on the accumulator (see
//! `c6_fold`).

use bb_proof_verification::{
    Ultra(A)onkProof, Ultra(A)onkVerificationKey, Ultra(A)onkZKProof, verify_honk_proof,
};
use lib::configs::default::T;

/// One C6 public row: `expected_sk`, `expected_e_sm`, `ct_commitment`, `d_commitment` (see C6 leaf).
pub global C6_PUBLIC_LEN: u32 = 4;

/// Same shape as `c6_fold`: 4 pub parameters + 4 columns of `T + 1` party slots each.
pub global C6_FOLD_PUBLIC_LEN: u32 = 4 + (4 * (T + 1));

fn main(
    inner_vk: Ultra(A)onkVerificationKey,
    inner_proof: Ultra(A)onkZKProof,
    c6_public_inputs: [Field; C6_PUBLIC_LEN],
    acc_vk: Ultra(A)onkVerificationKey,
    acc_proof: Ultra(A)onkProof,
    acc_public_inputs: [Field; C6_FOLD_PUBLIC_LEN],
    inner_key_hash: pub Field,
    acc_key_hash: pub Field,
    is_first_step: pub bool,
    slot_index: pub u32,
) -> pub ([Field; T + 1], [Field; T + 1], [Field; T + 1], [Field; T + 1]) {
    assert(slot_index < T + 1);

    verify_honk_proof(inner_vk, inner_proof, c6_public_inputs, inner_key_hash);

    // Signature parity with `c6_fold`: see `c3_fold_kernel`.
    let _ = (acc_vk, acc_proof, acc_key_hash, acc_public_inputs);

    assert(is_first_step);

    let sk_new = c6_public_inputs[0];
    let esm_new = c6_public_inputs[1];
    let ct_new = c6_public_inputs[2];
    let d_new = c6_public_inputs[3];

    let mut out_sk: [Field; T + 1] = [0; T + 1];
    let mut out_esm: [Field; T + 1] = [0; T + 1];
    let mut out_ct: [Field; T + 1] = [0; T + 1];
    let mut out_d: [Field; T + 1] = [0; T + 1];

    for i in 0..(T + 1) {
        let idx = i as u32;
        if idx == slot_index {
            out_sk[i] = sk_new;
            out_esm[i] = esm_new;
            out_ct[i] = ct_new;
            out_d[i] = d_new;
        } else {
            out_sk[i] = 0;
            out_esm[i] = 0;
            out_ct[i] = 0;
            out_d[i] = 0;
        }
    }

    (out_sk, out_esm, out_ct, out_d)
}
