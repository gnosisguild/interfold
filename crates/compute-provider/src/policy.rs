// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! How an E3 program's inputs become tree leaves, and which of them the computation sees.
//!
//! Both are program-specific. A leaf must match whatever the E3 program builds on chain, and no
//! two programs need agree; selection is the program's answer to "what does a second input for the
//! same participant mean?", which CRISP answers differently from a program where every input
//! counts.
//!
//! What is **not** program-specific, and is enforced by this crate rather than delegated:
//!
//! - leaves are derived from the ciphertexts the Secure Process consumed, never received alongside
//!   them — a received root can disagree with the data it claims to describe;
//! - every published input contributes a leaf, whatever the policy decides about computing over it
//!   — dropping one changes the root and makes the result unpublishable.

use crate::compute_input::ComputeError;
use num_bigint::BigUint;
use num_traits::Num;

/// The BN254 scalar field. Every leaf must reduce into it before the tree will accept it.
pub const SNARK_SCALAR_FIELD: &str =
    "21888242871839275222246405745257275088548364400416034343698204186575808495617";

/// One published input, as the Secure Process sees it.
pub struct PublishedInput<'a> {
    /// Position in the input set, which is also this leaf's position in the tree.
    pub index: usize,
    /// The serialized ciphertext the E3 program published.
    pub ciphertext: &'a [u8],
    /// The commitment the E3 program stored, when it publishes one.
    ///
    /// The proof an E3 program checks at input time typically constrains this and never sees the
    /// serialized bytes, so the two can disagree. Comparing it against `recomputed` is the only
    /// place that mismatch can be detected.
    pub commitment: Option<&'a [u8; 32]>,
    /// Whatever else the E3 program published per input, opaque to this crate.
    ///
    /// CRISP carries the 20-byte slot address here, because its tree is append-only and it selects
    /// per slot. A program with no such notion leaves this empty.
    pub metadata: &'a [u8],
    /// The commitment recomputed from `ciphertext`, or `None` when it does not deserialize.
    pub recomputed: Option<[u8; 32]>,
}

impl PublishedInput<'_> {
    /// Whether the published bytes reproduce the commitment the E3 program stored.
    ///
    /// `false` when the two disagree or the ciphertext does not deserialize. Always `true` when
    /// the program publishes no commitment, since there is then nothing to check against.
    pub fn matches_commitment(&self) -> bool {
        match (self.commitment, self.recomputed) {
            (Some(stored), Some(recomputed)) => *stored == recomputed,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

/// Builds a leaf, which must equal what the E3 program builds on chain for the same input.
pub type LeafFn = fn(&PublishedInput) -> Result<String, ComputeError>;

/// Chooses which inputs the computation runs over, by index.
///
/// Must be a function of data the input root binds, or two provers over the same published inputs
/// would disagree and a prover could choose what to leave out.
pub type SelectFn = fn(&[PublishedInput]) -> Vec<usize>;

/// An E3 program's answers to both questions.
#[derive(Clone, Copy)]
pub struct InputPolicy {
    pub leaf: LeafFn,
    pub select: SelectFn,
}

impl Default for InputPolicy {
    /// The behaviour every E3 program had before policies existed: the leaf is the ciphertext's
    /// own commitment, and every input is computed over.
    fn default() -> Self {
        Self {
            leaf: commitment_leaf,
            select: all_inputs,
        }
    }
}

/// A leaf that is the ciphertext's own SAFE commitment.
///
/// Matches an E3 program that inserts the commitment directly, as the starter template does. It
/// cannot detect a published ciphertext that disagrees with its commitment, because the leaf is
/// derived from the bytes alone — a program that needs that must bind both, as CRISP does.
pub fn commitment_leaf(input: &PublishedInput) -> Result<String, ComputeError> {
    input
        .recomputed
        .map(hex::encode)
        .ok_or_else(|| ComputeError::LeafCommitment {
            index: input.index,
            reason: "ciphertext could not be deserialized".to_string(),
        })
}

/// Every input is computed over, in published order.
pub fn all_inputs(inputs: &[PublishedInput]) -> Vec<usize> {
    (0..inputs.len()).collect()
}

/// Reduces a digest into the scalar field and hex-encodes it, ready to be a leaf.
///
/// Exposed because reducing into BN254 is a property of the tree rather than of any one program,
/// so a program building its own leaf should not have to restate the modulus.
pub fn leaf_from_digest(digest: &[u8]) -> String {
    let field = BigUint::from_str_radix(SNARK_SCALAR_FIELD, 10).expect("field constant");
    hex::encode((BigUint::from_bytes_be(digest) % field).to_bytes_be())
}
