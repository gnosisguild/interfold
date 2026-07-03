// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! C2a/C2b (ShareComputation) → C4a/C4b (ShareDecryption)
//! share-commitment consistency links.
//!
//! ## Purpose
//!
//! Each C2 proof outputs per-party-per-modulus share commitments via
//! `commit_to_party_shares`. The aggregator's C4 proof must list those same
//! values in its `expected_commitments` input array. This link verifies that
//! every share computed in C2 has a matching decryption expectation in C4.
//!
//! ## Direction
//!
//! Source is C2 (the sender's share-computation proof), target is C4 (the
//! recipient/aggregator's share-decryption proof). C2 and C4 are produced by
//! different parties. Fault is attributed to the C2 sender if its L share
//! commitments for the C4 recipient do not exactly match the corresponding
//! row in C4's `expected_commitments`.
//!
//! ## C2 inner-circuit public signals layout
//!
//! ```text
//! [expected_secret_commitment (32 B, skip)]
//! [party_0_mod_0 (32 B)] [party_0_mod_1] ... [party_0_mod_{L-1}]
//! [party_1_mod_0] ...
//! [party_{N-1}_mod_{L-1}]
//! ```
//!
//! The first field is the `expected_secret_commitment` public input and is
//! skipped. The remaining N_PARTIES × L fields are share commitments output
//! by `commit_to_party_shares`, indexed in row-major order (party first, then
//! modulus).
//!
//! ## C4 public signals layout
//!
//! ```text
//! [expected_commitments[0][0] (32 B)] ... [expected_commitments[0][L-1]]
//! [expected_commitments[1][0]] ...
//! [expected_commitments[H-1][L-1]]
//! [commitment (32 B, TAIL aggregated output)]
//! ```
//!
//! ## Precise check
//!
//! Given:
//! - `src_party_id` = C2 sender's 0-based committee index (= X)
//! - `tgt_party_id` = C4 recipient's 0-based committee index (= R)
//!
//! The L commitments from C2 at slot R (`source_values[R*L .. (R+1)*L]`)
//! must exactly match C4's row X (`expected_commitments[X][0..L]`).
//! This verifies all L moduli, not just one.
//!
//! ## Scope
//!
//! `SourceMustExistInTargets` — C2 is produced by the sender, C4 by the
//! aggregator/recipient; they are different parties. Fault is attributed to C2
//! if its L share commitments for the C4 recipient do not appear at the correct
//! row in any C4 proof.

use super::{CommitmentLink, FieldValue, LinkScope};
use e3_events::ProofType;
use e3_zk_helpers::FIELD_BYTE_LEN;

/// C2a (SkShareComputation) → C4a (SkShareDecryption) commitment link.
pub struct C2aToC4aShareCommitmentLink {
    /// Number of threshold CRT moduli (L). Determines the block size in both
    /// C2 and C4 public signals.
    pub l: usize,
}

impl CommitmentLink for C2aToC4aShareCommitmentLink {
    fn name(&self) -> &'static str {
        "C2a->C4a share commitments"
    }

    fn source_proof_type(&self) -> ProofType {
        ProofType::C2aSkShareComputation
    }

    fn target_proof_type(&self) -> ProofType {
        ProofType::C4aSkShareDecryption
    }

    fn scope(&self) -> LinkScope {
        LinkScope::SourceMustExistInTargets
    }

    fn extract_source_values(&self, public_signals: &[u8]) -> Vec<FieldValue> {
        extract_share_commitments(public_signals)
    }

    fn check_consistency(
        &self,
        source_values: &[FieldValue],
        target_public_signals: &[u8],
        src_party_id: u64,
        tgt_party_id: u64,
    ) -> bool {
        check_exact_l_commitments(
            source_values,
            target_public_signals,
            src_party_id,
            tgt_party_id,
            self.l,
        )
    }
}

/// C2b (ESmShareComputation) → C4b (ESmShareDecryption) commitment link.
pub struct C2bToC4bShareCommitmentLink {
    /// Number of threshold CRT moduli (L).
    pub l: usize,
}

impl CommitmentLink for C2bToC4bShareCommitmentLink {
    fn name(&self) -> &'static str {
        "C2b->C4b share commitments"
    }

    fn source_proof_type(&self) -> ProofType {
        ProofType::C2bESmShareComputation
    }

    fn target_proof_type(&self) -> ProofType {
        ProofType::C4bESmShareDecryption
    }

    fn scope(&self) -> LinkScope {
        LinkScope::SourceMustExistInTargets
    }

    fn extract_source_values(&self, public_signals: &[u8]) -> Vec<FieldValue> {
        extract_share_commitments(public_signals)
    }

    fn check_consistency(
        &self,
        source_values: &[FieldValue],
        target_public_signals: &[u8],
        src_party_id: u64,
        tgt_party_id: u64,
    ) -> bool {
        check_exact_l_commitments(
            source_values,
            target_public_signals,
            src_party_id,
            tgt_party_id,
            self.l,
        )
    }
}

/// Extract all share commitments from C2's public signals.
///
/// C2 inner-circuit public signals:
///   - field 0: `expected_secret_commitment` (skipped)
///   - fields 1..: `commit_to_party_shares[party_idx][mod_idx]` outputs,
///     row-major (party first, modulus second)
///
/// Returns every 32-byte chunk after the first field.
fn extract_share_commitments(public_signals: &[u8]) -> Vec<FieldValue> {
    if public_signals.len() < 2 * FIELD_BYTE_LEN {
        return vec![];
    }
    public_signals[FIELD_BYTE_LEN..]
        .chunks(FIELD_BYTE_LEN)
        .filter_map(|chunk| {
            if chunk.len() == FIELD_BYTE_LEN {
                let mut value = [0u8; FIELD_BYTE_LEN];
                value.copy_from_slice(chunk);
                Some(value)
            } else {
                None
            }
        })
        .collect()
}

/// Precise L-way check: verifies that the L share commitments C2_X computed
/// for recipient R exactly match C4_R's expected_commitments row for sender X.
///
/// - `source_values`: all N_PARTIES × L commits from C2_X (from `extract_share_commitments`)
/// - `target_public_signals`: C4_R's public signals
/// - `src_party_id`: C2 sender X (0-based committee index)
/// - `tgt_party_id`: C4 recipient R (0-based committee index)
/// - `l`: number of CRT moduli
///
/// Extracts `source_values[R*L .. (R+1)*L]` and checks it equals
/// `target_public_signals[X*L*32 .. (X+1)*L*32]`.
fn check_exact_l_commitments(
    source_values: &[FieldValue],
    target_public_signals: &[u8],
    src_party_id: u64,
    tgt_party_id: u64,
    l: usize,
) -> bool {
    if source_values.is_empty() || l == 0 {
        return false;
    }

    let tgt_idx = tgt_party_id as usize;
    let src_idx = src_party_id as usize;

    // Slice L commits from C2 at slot tgt_idx (the C4 recipient's position).
    let c2_start = tgt_idx * l;
    let c2_end = c2_start + l;
    if source_values.len() < c2_end {
        return false;
    }
    let c2_block = &source_values[c2_start..c2_end];

    // C4 row for src_idx (the C2 sender): bytes [X*L*32 .. (X+1)*L*32].
    // C4 must also have the aggregated output as the last field.
    let c4_row_start = src_idx * l * FIELD_BYTE_LEN;
    let c4_row_end = c4_row_start + l * FIELD_BYTE_LEN;
    if target_public_signals.len() < c4_row_end + FIELD_BYTE_LEN {
        return false;
    }

    // Verify all L commitments match exactly.
    c2_block.iter().enumerate().all(|(i, expected)| {
        let offset = c4_row_start + i * FIELD_BYTE_LEN;
        &target_public_signals[offset..offset + FIELD_BYTE_LEN] == expected.as_slice()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_field(val: u8) -> [u8; 32] {
        let mut f = [0u8; 32];
        f[31] = val;
        f
    }

    /// C2 inner signals: [expected_secret_commitment] + share commitments (row-major: party, mod).
    fn c2_signals(share_commits: &[[u8; 32]]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&make_field(0xFF)); // expected_secret_commitment (skipped)
        for c in share_commits {
            v.extend_from_slice(c);
        }
        v
    }

    /// C4 signals: [expected_commitments row-major (party, mod)..., aggregated_commitment].
    fn c4_signals(rows: &[Vec<[u8; 32]>], aggregated: [u8; 32]) -> Vec<u8> {
        let mut v = Vec::new();
        for row in rows {
            for c in row {
                v.extend_from_slice(c);
            }
        }
        v.extend_from_slice(&aggregated);
        v
    }

    #[test]
    fn extract_share_commitments_from_c2() {
        let link = C2aToC4aShareCommitmentLink { l: 2 };
        // C2 with 3 parties × 2 moduli = 6 share commits
        let commits: Vec<[u8; 32]> = (1u8..=6).map(make_field).collect();
        let c2 = c2_signals(&commits);
        let values = link.extract_source_values(&c2);
        assert_eq!(values.len(), 6);
        assert_eq!(values[0], make_field(1));
        assert_eq!(values[5], make_field(6));
    }

    #[test]
    fn extract_skips_secret_commitment() {
        let link = C2aToC4aShareCommitmentLink { l: 2 };
        let c2 = c2_signals(&[make_field(1), make_field(2)]);
        let values = link.extract_source_values(&c2);
        assert_eq!(values.len(), 2);
        assert!(!values.contains(&make_field(0xFF)));
    }

    /// 3 parties (N=3), 2 moduli (L=2), 2 authorized parties (H=2).
    /// C2 from party X=1, C4 for recipient R=1.
    /// C2_X: [p0m0,p0m1, p1m0,p1m1, p2m0,p2m1]
    /// C4_R=1: rows for party 0 and party 1 = [[p0m0,p0m1],[p1m0,p1m1]] + agg
    /// check_consistency(src=1, tgt=1): C2 slot R=1 = [p1m0,p1m1] must equal C4 row X=1 = [p1m0,p1m1]
    #[test]
    fn consistency_passes_precise_l_way_check() {
        let l = 2;
        let link = C2aToC4aShareCommitmentLink { l };

        // C2 from sender X=1: 3 parties × 2 moduli
        let c2 = c2_signals(&[
            make_field(10),
            make_field(11), // party 0
            make_field(20),
            make_field(21), // party 1 (slot for tgt_party=1)
            make_field(30),
            make_field(31), // party 2
        ]);
        let source_values = link.extract_source_values(&c2);

        // C4 for recipient R=1: 2 authorized parties (rows for X=0 and X=1)
        let c4 = c4_signals(
            &[
                vec![make_field(10), make_field(11)], // row X=0
                vec![make_field(20), make_field(21)], // row X=1 (sender's commits for this recipient)
            ],
            make_field(99), // aggregated output
        );

        // src_party_id=1 (sender X=1), tgt_party_id=1 (recipient R=1)
        assert!(link.check_consistency(&source_values, &c4, 1, 1));
    }

    #[test]
    fn consistency_fails_when_wrong_modulus_commitment() {
        let l = 2;
        let link = C2aToC4aShareCommitmentLink { l };

        let c2 = c2_signals(&[
            make_field(10),
            make_field(11),
            make_field(20),
            make_field(21), // party 1 slot
            make_field(30),
            make_field(31),
        ]);
        let source_values = link.extract_source_values(&c2);

        // C4 has correct first modulus (20) but wrong second (99 instead of 21)
        let c4 = c4_signals(
            &[
                vec![make_field(10), make_field(11)],
                vec![make_field(20), make_field(99)], // second modulus wrong
            ],
            make_field(0),
        );

        assert!(!link.check_consistency(&source_values, &c4, 1, 1));
    }

    #[test]
    fn consistency_fails_when_wrong_party_slot() {
        let l = 2;
        let link = C2aToC4aShareCommitmentLink { l };

        let c2 = c2_signals(&[
            make_field(10),
            make_field(11), // party 0
            make_field(20),
            make_field(21), // party 1
        ]);
        let source_values = link.extract_source_values(&c2);

        // C4 has party-0 commits in row 0 only
        let c4 = c4_signals(&[vec![make_field(10), make_field(11)]], make_field(0));

        // src=0, tgt=1: C2 slot 1 = [20,21], C4 row 0 = [10,11] — mismatch
        assert!(!link.check_consistency(&source_values, &c4, 0, 1));
    }

    #[test]
    fn consistency_does_not_match_aggregated_output() {
        let l = 1;
        let link = C2aToC4aShareCommitmentLink { l };

        // C2: 1 party × 1 modulus = commit 99
        let c2 = c2_signals(&[make_field(99)]);
        let source_values = link.extract_source_values(&c2);

        // C4: row 0 = [5], aggregated output = 99
        // commit 99 is only in the tail — must not match
        let c4 = c4_signals(&[vec![make_field(5)]], make_field(99));

        assert!(!link.check_consistency(&source_values, &c4, 0, 0));
    }

    #[test]
    fn short_or_empty_signals() {
        let link = C2aToC4aShareCommitmentLink { l: 2 };
        assert!(link.extract_source_values(&[0u8; 32]).is_empty());
        assert!(!link.check_consistency(&[], &[0u8; 256], 0, 0));
        assert!(!link.check_consistency(&[make_field(1)], &[0u8; 16], 0, 0));
    }

    #[test]
    fn c2b_to_c4b_variant() {
        let l = 2;
        let link = C2bToC4bShareCommitmentLink { l };
        let c2 = c2_signals(&[make_field(7), make_field(8)]);
        let source_values = link.extract_source_values(&c2);

        let c4 = c4_signals(&[vec![make_field(7), make_field(8)]], make_field(0));
        assert!(link.check_consistency(&source_values, &c4, 0, 0));

        let c4_wrong = c4_signals(&[vec![make_field(7), make_field(9)]], make_field(0));
        assert!(!link.check_consistency(&source_values, &c4_wrong, 0, 0));
    }
}
