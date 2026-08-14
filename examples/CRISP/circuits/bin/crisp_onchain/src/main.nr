// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crisp_lib::ciphertext_addition::CiphertextAddition;
use crisp_lib::ecdsa::{address_to_field, derive_address, digest_from_halves, validate_signature};
use crisp_lib::utils::{check_coefficient_values_with_balance, check_coefficient_zero};
use interfold_lib::configs::default::threshold::{
    L, N, Q_MOD_T_CENTERED, QIS, USER_DATA_ENCRYPTION_BIT_CT, USER_DATA_ENCRYPTION_BIT_K,
};
use interfold_lib::math::{
    commitments::{
        compute_ciphertext_commitment, compute_single_polynomial_commitment,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    },
    polynomial::Polynomial,
};

/// Main circuit function for CRISP vote verification, on-chain eligibility variant.
///
/// This circuit does the same work as the `crisp` circuit, but it takes the voting power of the
/// slot from the contract instead of from a census Merkle tree. The contract reads the voting
/// power from the token at the snapshot of the round, and supplies it as a public input.
///
/// The `crisp` circuit is for a census that the contract cannot compute, such as an address
/// allowlist. This circuit is for a census that comes from an on-chain token. Removing the tree
/// removes the trusted census producer, and it removes the Merkle path from each proof.
///
/// The circuit handles two use cases:
/// 1. **Actual Voting**: An eligible voter casts a vote
/// 2. **Mask Vote**: Anyone submits a zero vote to mask slot activity
///
/// Votes are stored in an on-chain Merkle tree, where each vote ciphertext is stored at a specific
/// index. A mapping stores the slot address (Ethereum address) to the index in the Merkle tree.
///   - For actual votes: the slot address must match the address derived from the signature
///   - For mask votes: the slot address can be any address (no address match required)
/// In both cases, the contract must confirm that the slot has voting power before it accepts the
/// input. The circuit cannot make that check, because it has no view of the token.
///
/// Mask votes prevent receipt sharing attacks by creating ambiguity: anyone can submit a zero vote
/// to any slot, creating a different ciphertext that decrypts to the same value. This makes it
/// impossible for a voter to prove which ciphertext was their actual vote, preventing them from
/// providing a verifiable receipt to a coercer or briber.
///
/// The circuit verifies all inputs and returns the appropriate ciphertext commitment.
/// Specifically, it returns a Field commitment depending on the case:
/// - **Actual vote**: `ct_commitment` - commitment to the new vote ciphertext
/// - **Mask vote (first vote)**: `ct_commitment` - commitment to the zero ciphertext
/// - **Mask vote (updating slot)**: `sum_ct_commitment` - commitment to the sum of previous votes in slot
///
fn main(
    // Ciphertext Addition Section
    prev_ct0is: [Polynomial<N>; L],
    prev_ct1is: [Polynomial<N>; L],
    prev_ct_commitment: pub Field,
    sum_ct0is: [Polynomial<N>; L],
    sum_ct1is: [Polynomial<N>; L],
    sum_r0is: [Polynomial<N>; L],
    sum_r1is: [Polynomial<N>; L],
    // User Data Encryption Section.
    ct0is: [Polynomial<N>; L],
    ct1is: [Polynomial<N>; L],
    k1: Polynomial<N>,
    // ECDSA Section.
    public_key_x: [u8; 32],
    public_key_y: [u8; 32],
    signature: [u8; 64],
    digest_hi: pub Field,
    digest_lo: pub Field,
    // Eligibility Section. The contract supplies both values.
    slot_address: pub Field,
    voting_power: pub Field,
    // Other inputs.
    is_first_vote: pub bool,
    is_mask_vote: bool,
    num_options: pub u32,
) -> pub (Field, Field, Field) {
    // ============================================================================
    // STEP 1: Pre-compute Commitments
    // ============================================================================
    // Compute commitments for ciphertext and plaintext that link this circuit to the
    // user_data_encryption circuit proofs in the fold circuit, where the 2 recursive
    // proofs are verified.
    let ct_commitment =
        compute_ciphertext_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(ct0is, ct1is);
    let k1_commitment = compute_single_polynomial_commitment::<N, USER_DATA_ENCRYPTION_BIT_K>(
        k1,
        DS_USER_DATA_ENCRYPTION_COMMITMENT,
    );

    // ============================================================================
    // STEP 2: Eligibility
    // ============================================================================
    // There is no Merkle proof in this circuit. `slot_address` and `voting_power` are public
    // inputs that the contract sets from the token at the snapshot of the round, so the prover
    // cannot change either value.
    //
    // The contract must reject an input when the slot has no voting power. That check applies to
    // actual votes and to mask votes, and it replaces the tree membership proof of the `crisp`
    // circuit.

    // ============================================================================
    // STEP 3: Vote Type Detection and Return Logic
    // ============================================================================
    // The circuit branches into two cases based on the is_mask_vote flag:
    //
    // CASE 1: ACTUAL VOTE (is_mask_vote == false)
    //   - This is an eligible voter casting a vote
    //   - Verify vote amount <= voting_power (checked via valid_vote)
    //   - Validate the signature over the public digest (checked via valid_signature)
    //     to authenticate the voter for this ballot
    //   - Verify address matches slot address (checked via valid_address)
    //   - Return new vote ciphertext commitment (ct_commitment)
    let valid_vote =
        check_coefficient_values_with_balance(k1, Q_MOD_T_CENTERED, voting_power, num_options);

    // The digest is a public input, so the prover cannot select the message that the signature
    // covers. The contract computes the digest from the round, the slot, and this ballot, which
    // binds the signature to all three.
    let hashed_message = digest_from_halves(digest_hi, digest_lo);
    let valid_signature = validate_signature(hashed_message, public_key_x, public_key_y, signature);
    let valid_address =
        slot_address == address_to_field(derive_address(public_key_x, public_key_y));

    // CASE 2: MASK VOTE (is_mask_vote == true)
    //   - This is anyone submitting a zero vote to mask slot activity
    //   - Verify vote is zero: k1 must be zero (checked via valid_zero_vote)
    //   - If first vote in slot: return zero ciphertext commitment (ct_commitment)
    //   - If updating slot (is_first_vote == false):
    //     * Verify prev_ct_commitment matches the computed commitment from (prev_ct0is, prev_ct1is)
    //       to ensure the prover hasn't tampered with the previous ciphertext
    //     * Verify ciphertext addition: prove that sum_ct = prev_ct + ct where:
    //       - prev_ct = (prev_ct0is, prev_ct1is) is the previous ciphertext in the slot
    //       - ct = (ct0is, ct1is) is the zero vote ciphertext being added
    //       - sum_ct = (sum_ct0is, sum_ct1is) is the resulting ciphertext
    //     * The verification uses commitments and Fiat-Shamir transform with Schwartz-Zippel
    //       lemma to prove the homomorphic addition without decrypting
    //     * Mask votes add zero to the previous ciphertext, creating a different ciphertext
    //       with the same plaintext, preventing receipt sharing attacks
    //     * Return sum ciphertext commitment (sum_ct_commitment)
    let sum_ct_commitment =
        compute_ciphertext_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(sum_ct0is, sum_ct1is);
    let _prev_ct_commitment =
        compute_ciphertext_commitment::<N, L, USER_DATA_ENCRYPTION_BIT_CT>(prev_ct0is, prev_ct1is);

    let valid_zero_vote = check_coefficient_zero(k1, num_options);
    let valid_prev_ct = prev_ct_commitment == _prev_ct_commitment;
    let ct_add: CiphertextAddition<N, L, USER_DATA_ENCRYPTION_BIT_CT, USER_DATA_ENCRYPTION_BIT_CT, USER_DATA_ENCRYPTION_BIT_CT> = CiphertextAddition::new(
        QIS,
        ct0is,
        ct1is,
        ct_commitment,
        prev_ct0is,
        prev_ct1is,
        prev_ct_commitment,
        sum_ct0is,
        sum_ct1is,
        sum_ct_commitment,
        sum_r0is,
        sum_r1is,
    );
    let valid_ct_add = ct_add.execute();

    // Actual conditional logic.
    if is_mask_vote == false {
        assert(valid_vote);
        assert(valid_signature);
        assert(valid_address);

        (ct_commitment, ct_commitment, k1_commitment)
    } else {
        assert(valid_zero_vote);

        if is_first_vote {
            (ct_commitment, ct_commitment, k1_commitment)
        } else {
            assert(valid_prev_ct);
            assert(valid_ct_add);

            (sum_ct_commitment, ct_commitment, k1_commitment)
        }
    }
}
