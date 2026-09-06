// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Round A combine, level 2 (P3): verifies `ct0_ab_combine` and `ct0_cd_combine`. Both inputs are
//! themselves already-aggregated, non-sensitive commitment values (not raw witness), so both are
//! proven non-ZK and verified here via `verify_honk_proof_non_zk`, matching how DKG's `finalize`
//! consumes `chunk_batch`'s non-ZK output.

use bb_proof_verification::{UltraHonkProof, UltraHonkVerificationKey, verify_honk_proof_non_zk};

fn main(
    ab_vk: UltraHonkVerificationKey,
    ab_proof: UltraHonkProof,
    ab_public_inputs: [Field; 3],
    ab_key_hash: pub Field,
    cd_vk: UltraHonkVerificationKey,
    cd_proof: UltraHonkProof,
    cd_public_inputs: [Field; 2],
    cd_key_hash: pub Field,
) -> pub (Field, Field, Field, Field, Field) {
    verify_honk_proof_non_zk(ab_vk, ab_proof, ab_public_inputs, ab_key_hash);
    verify_honk_proof_non_zk(cd_vk, cd_proof, cd_public_inputs, cd_key_hash);

    (
        ab_public_inputs[0], ab_public_inputs[1], ab_public_inputs[2], cd_public_inputs[0],
        cd_public_inputs[1],
    )
}
