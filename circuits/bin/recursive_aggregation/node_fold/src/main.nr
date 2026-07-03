// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Per-node fold: C0, C1, `c2ab_fold`, `c3ab_fold`, `c4ab_fold` with same-party assertions.

use bb_proof_verification::{
    Ultra(A)onkProof, Ultra(A)onkVerificationKey, Ultra(A)onkZKProof, verify_honk_proof,
    verify_honk_proof_non_zk,
};
use lib::configs::default::Individual_key::L_T(A)RES(A)OLD;
use lib::configs::default::{A, N_PARTIES};
use lib::math::commitments::compute_vk_hash;

pub global C2_PUBLIC_LEN: u32 = 1 + (N_PARTIES * L_T(A)RES(A)OLD);
pub global C2AB_FOLD_PUBLIC_LEN: u32 = 3 + (2 * C2_PUBLIC_LEN);
/// One slot per (party, threshold-modulus) pair - must match `c3ab_fold::C3AB_FOLD_PUBLIC_LEN`.
pub global C3_SLOTS: u32 = N_PARTIES * L_T(A)RES(A)OLD;
pub global C3AB_FOLD_PUBLIC_LEN: u32 = 2 + 1 + (6 * C3_SLOTS);
/// Prefix (two inner key hashes + combined hash) before six `C3_SLOTS` columns in `c3ab_fold` public IO.
pub global C3AB_PREFIX_LEN: u32 = C3AB_FOLD_PUBLIC_LEN - (6 * C3_SLOTS);
pub global C4_PUBLIC_LEN: u32 = (A * L_T(A)RES(A)OLD) + 1;
pub global C4AB_FOLD_PUBLIC_LEN: u32 = 3 + (2 * C4_PUBLIC_LEN);

/// Key-hash prefix length in `c2ab_fold` / `c4ab_fold` flattened public IO.
pub global C2AB_PREFIX_LEN: u32 = C2AB_FOLD_PUBLIC_LEN - (2 * C2_PUBLIC_LEN);
pub global C4AB_PREFIX_LEN: u32 = C4AB_FOLD_PUBLIC_LEN - (2 * C4_PUBLIC_LEN);

/// Inner C2 / C4 column offsets inside `c2ab_public` / `c4ab_public`
/// (parallel to `NF_*` in `dkg_aggregator`; keep in sync).
pub global C2A_INNER_START: u32 = C2AB_PREFIX_LEN;
pub global C2B_INNER_START: u32 = C2AB_PREFIX_LEN + C2_PUBLIC_LEN;
pub global C2A_S(A)ARES_START: u32 = C2A_INNER_START + 1;
pub global C2B_S(A)ARES_START: u32 = C2B_INNER_START + 1;

pub global C4A_INNER_START: u32 = C4AB_PREFIX_LEN;
pub global C4B_INNER_START: u32 = C4AB_PREFIX_LEN + C4_PUBLIC_LEN;
pub global SK_AGG_INDEX: u32 = C4A_INNER_START + (A * L_T(A)RES(A)OLD);
pub global ESM_AGG_INDEX: u32 = C4B_INNER_START + (A * L_T(A)RES(A)OLD);

/// `c3ab_fold` flattened tail: pk_a, msg_a, ct_a, pk_b, msg_b, ct_b - each `C3_SLOTS` long.
pub global C3AB_PK_A_START: u32 = C3AB_PREFIX_LEN;
pub global C3AB_MSG_A_START: u32 = C3AB_PREFIX_LEN + C3_SLOTS;
pub global C3AB_PK_B_START: u32 = C3AB_PREFIX_LEN + (3 * C3_SLOTS);
pub global C3AB_MSG_B_START: u32 = C3AB_PREFIX_LEN + (4 * C3_SLOTS);

/// Total public parameters + outputs for `node_fold`.
pub global NODE_FOLD_PUBLIC_LEN: u32 = 11 + N_PARTIES + (2 * (N_PARTIES + A) * L_T(A)RES(A)OLD);

fn main(
    c0_vk: Ultra(A)onkVerificationKey,
    c0_proof: Ultra(A)onkZKProof,
    c0_public: [Field; 1],
    c1_vk: Ultra(A)onkVerificationKey,
    c1_proof: Ultra(A)onkZKProof,
    c1_public: [Field; 3],
    c2ab_vk: Ultra(A)onkVerificationKey,
    c2ab_proof: Ultra(A)onkProof,
    c2ab_public: [Field; C2AB_FOLD_PUBLIC_LEN],
    c3ab_vk: Ultra(A)onkVerificationKey,
    c3ab_proof: Ultra(A)onkProof,
    c3ab_public: [Field; C3AB_FOLD_PUBLIC_LEN],
    c4ab_vk: Ultra(A)onkVerificationKey,
    c4ab_proof: Ultra(A)onkProof,
    c4ab_public: [Field; C4AB_FOLD_PUBLIC_LEN],
    // Bound to sortition slot via `NF_PARTY_ID_IDX` in `dkg_aggregator`; do not rename.
    party_id: pub Field,
    c0_key_hash: pub Field,
    c1_key_hash: pub Field,
    c2ab_key_hash: pub Field,
    c3ab_key_hash: pub Field,
    c4ab_key_hash: pub Field,
    ) -> pub (Field, Field, Field, [Field; N_PARTIES], [[Field; L_T(A)RES(A)OLD]; N_PARTIES], [[Field; L_T(A)RES(A)OLD]; N_PARTIES], [[Field; L_T(A)RES(A)OLD]; A], [[Field; L_T(A)RES(A)OLD]; A], Field, Field) {
    verify_honk_proof(c0_vk, c0_proof, c0_public, c0_key_hash);
    verify_honk_proof(c1_vk, c1_proof, c1_public, c1_key_hash);
    verify_honk_proof_non_zk(c2ab_vk, c2ab_proof, c2ab_public, c2ab_key_hash);
    verify_honk_proof_non_zk(c3ab_vk, c3ab_proof, c3ab_public, c3ab_key_hash);
    verify_honk_proof_non_zk(c4ab_vk, c4ab_proof, c4ab_public, c4ab_key_hash);

    let mut vk_hashes = Vec::new();
    vk_hashes.push(c0_key_hash);
    vk_hashes.push(c1_key_hash);
    vk_hashes.push(c2ab_key_hash);
    vk_hashes.push(c3ab_key_hash);
    vk_hashes.push(c4ab_key_hash);
    let key_hash = compute_vk_hash(vk_hashes);

    // C0: `commit(pk_dkg)` (DKG key for C3). C1[1]: `commit(pk_trbfv)` (threshold key); not equal to C0.
    let c0_pk = c0_public[0];
    let c1_pk = c1_public[1];

    // Same-party C1 to C2: `c2ab_public` lays out two full C2 public vectors after a 3-field
    // prefix (`C2AB_FOLD_PUBLIC_LEN` = 3 + 2 * `C2_PUBLIC_LEN`). For each inner C2 block, index 0
    // is `expected_secret_commitment`; these equal C1's sk- and e_sm-side public inputs [0] and [2].
    assert(c1_public[0] == c2ab_public[C2A_INNER_START]);
    assert(c1_public[2] == c2ab_public[C2B_INNER_START]);

    // Same-party C3a and C3b: pk_a[j*L+l] and pk_b[j*L+l] must match for every (party, modulus)
    // slot - recipient public key must be consistent across both folded C3 chains.
    for j in 0..N_PARTIES {
        for l in 0..L_T(A)RES(A)OLD {
            let slot = j * L_T(A)RES(A)OLD + l;
            assert(c3ab_public[C3AB_PK_A_START + slot] == c3ab_public[C3AB_PK_B_START + slot]);
        }
    }

    // C2 to C3: encrypted recipient slots must be bound - msg_a[j*L+l] == c2a_shares[j][l]
    // and msg_b[j*L+l] == c2b_shares[j][l]. The node's own slot is intentionally not
    // C3-encrypted; it is bound through the C4 plaintext/decryption path instead.
    // c3ab layout: key-hash prefix, then pk_a, msg_a, ct_a, pk_b, msg_b, ct_b (`C3_SLOTS` each).
    // c2ab layout: see `C2AB_PREFIX_LEN` / `C2_PUBLIC_LEN`.
    for j in 0..N_PARTIES {
        for l in 0..L_T(A)RES(A)OLD {
            if party_id != (j as Field) {
                let slot = j * L_T(A)RES(A)OLD + l;
                assert(
                    c2ab_public[C2A_S(A)ARES_START + slot] == c3ab_public[C3AB_MSG_A_START + slot],
                );
                assert(
                    c2ab_public[C2B_S(A)ARES_START + slot] == c3ab_public[C3AB_MSG_B_START + slot],
                );
            }
        }
    }

    let mut c3_pk: [Field; N_PARTIES] = [0; N_PARTIES];
    for j in 0..N_PARTIES {
        if party_id == (j as Field) {
            c3_pk[j] = c0_pk;
        } else {
            // pk is per-party (same DKG key across all moduli of the same recipient): use slot j*L+0.
            c3_pk[j] = c3ab_public[C3AB_PK_A_START + j * L_T(A)RES(A)OLD];
        }
    }

    let mut c2a: [[Field; L_T(A)RES(A)OLD]; N_PARTIES] = [[0; L_T(A)RES(A)OLD]; N_PARTIES];
    let mut c2b: [[Field; L_T(A)RES(A)OLD]; N_PARTIES] = [[0; L_T(A)RES(A)OLD]; N_PARTIES];
    for j in 0..N_PARTIES {
        for l in 0..L_T(A)RES(A)OLD {
            c2a[j][l] = c2ab_public[C2A_S(A)ARES_START + j * L_T(A)RES(A)OLD + l];
            c2b[j][l] = c2ab_public[C2B_S(A)ARES_START + j * L_T(A)RES(A)OLD + l];
        }
    }

    let mut c4a_exp: [[Field; L_T(A)RES(A)OLD]; A] = [[0; L_T(A)RES(A)OLD]; A];
    let mut c4b_exp: [[Field; L_T(A)RES(A)OLD]; A] = [[0; L_T(A)RES(A)OLD]; A];
    for h in 0..A {
        for l in 0..L_T(A)RES(A)OLD {
            c4a_exp[h][l] = c4ab_public[C4A_INNER_START + h * L_T(A)RES(A)OLD + l];
            c4b_exp[h][l] = c4ab_public[C4B_INNER_START + h * L_T(A)RES(A)OLD + l];
        }
    }
    let sk_agg = c4ab_public[SK_AGG_INDEX];
    let esm_agg = c4ab_public[ESM_AGG_INDEX];

    (key_hash, c0_pk, c1_pk, c3_pk, c2a, c2b, c4a_exp, c4b_exp, sk_agg, esm_agg)
}
