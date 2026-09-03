// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Closes the IPA accumulator for two Chonk-backed C3 tubes and emits one ordinary C3 fold proof.

use bb_proof_verification::{
    RollupHonkProof, RollupHonkVerificationKey, verify_root_rolluphonk_proof,
};
use lib::configs::default::dkg::L_THRESHOLD;
use lib::configs::default::N_PARTIES;

/// Must match the C3 tube: one slot per (party, threshold-modulus) pair.
pub global C3_SLOTS: u32 = N_PARTIES * L_THRESHOLD;
pub global C3_CHUNK_COUNT: u32 = 2;
pub global C3_FOLD_PUBLIC_LEN: u32 = 6 + (3 * C3_SLOTS);
pub global C3_FOLD_PREFIX_LEN: u32 = C3_FOLD_PUBLIC_LEN - (3 * C3_SLOTS);

fn main(
    c3_vks: [RollupHonkVerificationKey; C3_CHUNK_COUNT],
    c3_proofs: [RollupHonkProof; C3_CHUNK_COUNT],
    c3_public: [[Field; C3_FOLD_PUBLIC_LEN]; C3_CHUNK_COUNT],
    c3_key_hash: Field,
    c3_fold_key_hash: Field,
    ) -> pub (Field, Field, Field, Field, Field, Field, [Field; C3_SLOTS], [Field; C3_SLOTS], [Field; C3_SLOTS]) {
    for i in 0..C3_CHUNK_COUNT {
        verify_root_rolluphonk_proof(c3_vks[i], c3_proofs[i], c3_public[i], c3_key_hash);

        // The chunk boundary only changes the final slot index. All other C3 transcript fields
        // must remain bound to the same leaf, tube, kernel, and fold circuits.
        assert(c3_public[i][1] == c3_key_hash);
        if i > 0 {
            assert(c3_public[i][0] == c3_public[0][0]);
            assert(c3_public[i][1] == c3_public[0][1]);
            assert(c3_public[i][2] == c3_public[0][2]);
            assert(c3_public[i][4] == c3_public[0][4]);
            assert(c3_public[i][5] == c3_public[0][5]);
        }
    }

    let mut pk: [Field; C3_SLOTS] = [0; C3_SLOTS];
    let mut msg: [Field; C3_SLOTS] = [0; C3_SLOTS];
    let mut ct: [Field; C3_SLOTS] = [0; C3_SLOTS];

    for i in 0..C3_CHUNK_COUNT {
        for j in 0..C3_SLOTS {
            let pk_new = c3_public[i][C3_FOLD_PREFIX_LEN + j];
            let msg_new = c3_public[i][C3_FOLD_PREFIX_LEN + C3_SLOTS + j];
            let ct_new = c3_public[i][C3_FOLD_PREFIX_LEN + C3_SLOTS + C3_SLOTS + j];
            if pk_new != 0 {
                assert(pk[j] == 0);
                pk[j] = pk_new;
                msg[j] = msg_new;
                ct[j] = ct_new;
            } else {
                assert(msg_new == 0);
                assert(ct_new == 0);
            }
        }
    }

    (
        c3_public[0][0],
        c3_fold_key_hash,
        c3_public[0][2],
        c3_public[C3_CHUNK_COUNT - 1][3],
        c3_public[0][4],
        c3_fold_key_hash,
        pk,
        msg,
        ct,
    )
}
