// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Pure validation for on-chain plaintext-output publication.
//!
//! The actor only performs the chain preflight + transaction once these
//! invariants hold; rejecting a malformed result is safer than a partial
//! on-chain write.

use e3_events::{E3id, Proof};
use e3_utils::utility_types::ArcBytes;
use std::time::Duration;

#[cfg(test)]
use e3_events::CircuitName;

/// Validate a decrypted result before it is written on-chain.
///
/// Returns `Ok(())` when exactly one decrypted output is present and the non-empty proof list
/// matches the output count.
/// Returns a human-readable error message otherwise.
pub(crate) fn validate_plaintext_output(
    e3_id: &E3id,
    decrypted_output: &[ArcBytes],
    decryption_aggregator_proofs: &[Proof],
) -> Result<(), String> {
    if decrypted_output.is_empty() {
        return Err("Decrypted output was empty!".to_string());
    }
    // Reject multi-output results — partial on-chain write is worse than failing.
    if decrypted_output.len() > 1 {
        return Err(format!(
            "E3 {} has {} decrypted outputs but only single-output is supported. \
            Refusing partial on-chain write.",
            e3_id,
            decrypted_output.len()
        ));
    }
    if decryption_aggregator_proofs.is_empty() {
        return Err(format!(
            "E3 {} has no decryption aggregator proof payload",
            e3_id
        ));
    }
    if decrypted_output.len() != decryption_aggregator_proofs.len() {
        return Err(format!(
            "E3 {} decrypted_output len ({}) != decryption_aggregator_proofs len ({})",
            e3_id,
            decrypted_output.len(),
            decryption_aggregator_proofs.len()
        ));
    }
    Ok(())
}

/// Return a deterministic wait for one committee member's deadline attempt.
/// A past deadline still keeps the party-id stagger after restart.
pub(crate) fn failure_watch_delay(
    now_unix_secs: u64,
    deadline_unix_secs: u64,
    party_id: u64,
    party_stagger_secs: u64,
) -> Duration {
    let deadline_wait = deadline_unix_secs
        .saturating_sub(now_unix_secs)
        .saturating_add(1);
    let stagger = party_id.saturating_mul(party_stagger_secs);
    Duration::from_secs(deadline_wait.saturating_add(stagger))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e3() -> E3id {
        E3id::new("1", 1)
    }

    fn bytes(n: usize) -> Vec<ArcBytes> {
        (0..n).map(|i| ArcBytes::from_bytes(&[i as u8])).collect()
    }

    fn proof() -> Proof {
        Proof::new(
            CircuitName::PkBfv,
            ArcBytes::from_bytes(&[0u8]),
            ArcBytes::from_bytes(&[0u8]),
        )
    }

    #[test]
    fn rejects_empty_output() {
        let err = validate_plaintext_output(&e3(), &[], &[]).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn rejects_multi_output() {
        let err = validate_plaintext_output(&e3(), &bytes(2), &[]).unwrap_err();
        assert!(err.contains("single-output"));
    }

    #[test]
    fn rejects_single_output_without_proof() {
        let err = validate_plaintext_output(&e3(), &bytes(1), &[]).unwrap_err();
        assert!(err.contains("no decryption aggregator proof"));
    }

    #[test]
    fn rejects_proof_count_mismatch() {
        let proofs = vec![proof(), proof()];
        let err = validate_plaintext_output(&e3(), &bytes(1), &proofs).unwrap_err();
        assert!(err.contains("!="));
    }

    #[test]
    fn accepts_matching_single_proof() {
        let proofs = vec![proof()];
        assert!(validate_plaintext_output(&e3(), &bytes(1), &proofs).is_ok());
    }

    #[test]
    fn failure_watch_stagger_survives_restart() {
        assert_eq!(failure_watch_delay(100, 160, 0, 15).as_secs(), 61);
        assert_eq!(failure_watch_delay(100, 160, 2, 15).as_secs(), 91);
        assert_eq!(failure_watch_delay(200, 160, 0, 15).as_secs(), 1);
        assert_eq!(failure_watch_delay(200, 160, 2, 15).as_secs(), 31);
    }
}
