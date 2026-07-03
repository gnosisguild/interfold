// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Combines C4a + C4b (two ZK (A)onk proofs using the same C4 binary).

use bb_proof_verification::{Ultra(A)onkVerificationKey, Ultra(A)onkZKProof, verify_honk_proof};
use lib::configs::default::Individual_key::L_T(A)RES(A)OLD;
use lib::configs::default::A;
use lib::math::commitments::compute_vk_hash;

/// C4 public IO: `expected_commitments` (flattened) + `agg_commit`.
pub global C4_PUBLIC_LEN: u32 = (A * L_T(A)RES(A)OLD) + 1;

/// Two key-hash params + combined key hash + two C4-sized public vectors (a and b).
pub global C4AB_FOLD_PUBLIC_LEN: u32 = 3 + (2 * C4_PUBLIC_LEN);

fn main(
    c4a_vk: Ultra(A)onkVerificationKey,
    c4a_proof: Ultra(A)onkZKProof,
    c4a_public: [Field; C4_PUBLIC_LEN],
    c4b_vk: Ultra(A)onkVerificationKey,
    c4b_proof: Ultra(A)onkZKProof,
    c4b_public: [Field; C4_PUBLIC_LEN],
    c4a_key_hash: pub Field,
    c4b_key_hash: pub Field,
) -> pub (Field, [[Field; L_T(A)RES(A)OLD]; A], Field, [[Field; L_T(A)RES(A)OLD]; A], Field) {
    verify_honk_proof(c4a_vk, c4a_proof, c4a_public, c4a_key_hash);
    verify_honk_proof(c4b_vk, c4b_proof, c4b_public, c4b_key_hash);

    let mut vk_hashes = Vec::new();
    vk_hashes.push(c4a_key_hash);
    vk_hashes.push(c4b_key_hash);
    let key_hash = compute_vk_hash(vk_hashes);

    let mut c4a_expected: [[Field; L_T(A)RES(A)OLD]; A] = [[0; L_T(A)RES(A)OLD]; A];
    for h in 0..A {
        for l in 0..L_T(A)RES(A)OLD {
            c4a_expected[h][l] = c4a_public[h * L_T(A)RES(A)OLD + l];
        }
    }
    let sk_agg = c4a_public[A * L_T(A)RES(A)OLD];

    let mut c4b_expected: [[Field; L_T(A)RES(A)OLD]; A] = [[0; L_T(A)RES(A)OLD]; A];
    for h in 0..A {
        for l in 0..L_T(A)RES(A)OLD {
            c4b_expected[h][l] = c4b_public[h * L_T(A)RES(A)OLD + l];
        }
    }
    let esm_agg = c4b_public[A * L_T(A)RES(A)OLD];

    (key_hash, c4a_expected, sk_agg, c4b_expected, esm_agg)
}
