// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::*;

#[actix::test]
async fn missing_c6_inner_proofs_emit_e3_failed() -> Result<()> {
    let (mut aggregator, history, e3_id) =
        build_plaintext_aggregator(generating_c7_state(), true).await?;
    aggregator.pending.c7_proofs_pending = Some(vec![dummy_proof(CircuitName::PkAggregation)]);
    aggregator.pending.honest_c6_proofs_for_agg = Some(vec![
        (0, vec![]),
        (1, vec![dummy_proof(CircuitName::ThresholdShareDecryption)]),
    ]);

    let ec = test_ctx(E3Failed {
        e3_id: e3_id.clone(),
        failed_at_stage: E3Stage::None,
        reason: FailureReason::None,
    });
    aggregator.dispatch_decryption_aggregation(&ec)?;

    let event = next_event(&history).await?;
    assert!(matches!(
        event.into_data(),
        InterfoldEventData::E3Failed(data)
            if data.e3_id == e3_id
                && data.failed_at_stage == E3Stage::CiphertextReady
                && data.reason == FailureReason::DecryptionInvalidShares
    ));
    assert!(aggregator.pending.honest_c6_proofs_for_agg.is_none());
    assert!(aggregator
        .pending
        .decryption_aggregation_correlation
        .is_none());
    assert!(aggregator.pending.c7_proofs_pending.is_none());
    assert!(aggregator.pending.decryption_aggregator_proofs.is_none());

    Ok(())
}

#[actix::test]
async fn proof_aggregation_disabled_marks_decryption_aggregator_ready() -> Result<()> {
    let (mut aggregator, _history, _e3_id) =
        build_plaintext_aggregator(generating_c7_state(), false).await?;
    aggregator.pending.c7_proofs_pending = Some(vec![dummy_proof(CircuitName::PkAggregation)]);
    let ec = test_ctx(E3Failed {
        e3_id: aggregator.e3_id.clone(),
        failed_at_stage: E3Stage::None,
        reason: FailureReason::None,
    });

    aggregator.dispatch_decryption_aggregation(&ec)?;
    assert!(aggregator
        .pending
        .decryption_aggregator_proofs
        .as_ref()
        .is_some_and(|p| p.is_empty()));
    assert!(aggregator
        .pending
        .decryption_aggregation_correlation
        .is_none());

    Ok(())
}
