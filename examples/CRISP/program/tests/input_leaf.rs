// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! The leaf layout, against the same vector `tests/input-leaf.test.ts` asserts in `crisp-contracts`.
//!
//! `CRISPProgram.inputLeaf` and [`e3_user_program::policy::leaf`] must agree byte for byte, or no
//! root the Secure Process derives will ever match the one the contract accumulated — and that
//! failure has no other symptom. Neither language can catch a divergence alone, so both check the
//! same vector and one of them fails when either side moves.

use e3_compute_provider::policy::PublishedInput;
use e3_user_program::policy::leaf;
use num_bigint::BigUint;

/// `0x` + the bytes 0..64, which is what the TypeScript vector spells out.
fn ciphertext() -> Vec<u8> {
    (0..64u8).collect()
}

const COMMITMENT: [u8; 32] = [0xab; 32];
const SLOT: [u8; 20] = [0xcd; 20];

/// `abi.encodePacked(address, uint40)`, as `CRISPProgram` lays the metadata out.
fn metadata(parent_index_plus_one: u64) -> Vec<u8> {
    let mut bytes = SLOT.to_vec();
    bytes.extend_from_slice(&parent_index_plus_one.to_be_bytes()[3..]);
    bytes
}

/// The leaf, as `leaf_from_digest` renders it: a hex string, already reduced into the field.
///
/// The TypeScript vector is the same number in decimal, because Solidity returns a `uint256`.
fn leaf_of(ciphertext: &[u8], commitment: &[u8; 32], metadata: &[u8]) -> String {
    leaf(&PublishedInput {
        index: 0,
        ciphertext,
        commitment: Some(commitment),
        metadata,
        recomputed: None,
    })
    .expect("the vector is well formed")
}

#[test]
fn the_leaf_matches_the_contract_vector() {
    let metadata = metadata(0);

    assert_eq!(
        leaf_of(&ciphertext(), &COMMITMENT, &metadata),
        "17910fb4333ca8c834bafea151e216e488db0fdaba00d7acd8d15a9e4a6ba9b4",
        "the Rust leaf diverged from the vector CRISPProgram.inputLeaf produces"
    );
}

/// The parent is bound, so an entry cannot be re-pointed at another one.
#[test]
fn the_leaf_changes_with_the_parent() {
    let first = leaf_of(&ciphertext(), &COMMITMENT, &metadata(0));
    let second = leaf_of(&ciphertext(), &COMMITMENT, &metadata(1));

    assert_ne!(first, second);
    assert_eq!(
        second, "098ef4852f0e5a3ae22bee86209ba3f653af2d34632ab68fadf1525af6b26d40",
        "the Rust leaf diverged from the vector CRISPProgram.inputLeaf produces"
    );
}

/// Every leaf has to reduce into the scalar field, or `LazyIMT` refuses it on chain.
#[test]
fn every_leaf_reduces_into_the_scalar_field() {
    let field = BigUint::parse_bytes(
        b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
        10,
    )
    .unwrap();

    for parent in 0..8u64 {
        let value = BigUint::parse_bytes(
            leaf_of(&ciphertext(), &COMMITMENT, &metadata(parent)).as_bytes(),
            16,
        )
        .expect("the leaf is a hex string");

        assert!(value < field);
    }
}

/// Metadata of the wrong length is refused rather than hashed, so an input that could never be
/// placed in a chain does not silently contribute a leaf the contract disagrees with.
#[test]
fn metadata_of_the_wrong_length_is_refused() {
    let bytes = ciphertext();
    let result = leaf(&PublishedInput {
        index: 0,
        ciphertext: &bytes,
        commitment: Some(&COMMITMENT),
        metadata: &SLOT,
        recomputed: None,
    });

    assert!(result.is_err(), "a 20-byte metadata predates the parent");
}
