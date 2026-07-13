// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::*;

#[actix::test]
async fn honest_dkg_fold_without_attestation_is_not_buffered() -> Result<()> {
    let correlation_id = CorrelationId::new();
    let mut initial_state = generating_c5_state(correlation_id);
    let PublicKeyAggregatorState::GeneratingC5Proof {
        ref mut party_nodes,
        ref mut honest_party_ids,
        ..
    } = initial_state
    else {
        unreachable!();
    };
    honest_party_ids.insert(2);
    party_nodes.insert(2, "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".to_string());

    let (mut aggregator, _history, e3_id) = build_public_key_aggregator(initial_state).await?;
    let ec = test_ctx(DKGRecursiveAggregationComplete {
        e3_id: e3_id.clone(),
        party_id: 2,
        aggregated_proof: Some(dummy_proof(CircuitName::NodeFold)),
        fold_attestation: None,
    });

    aggregator.handle_dkg_recursive_aggregation_complete(TypedEvent::new(
        DKGRecursiveAggregationComplete {
            e3_id: e3_id.clone(),
            party_id: 2,
            aggregated_proof: Some(dummy_proof(CircuitName::NodeFold)),
            fold_attestation: None,
        },
        ec,
    ))?;

    let Some(PublicKeyAggregatorState::GeneratingC5Proof {
        dkg_node_proofs, ..
    }) = aggregator.state.get()
    else {
        panic!("expected GeneratingC5Proof state");
    };
    assert!(!dkg_node_proofs.contains_key(&2));

    Ok(())
}

#[actix::test]
async fn pk_aggregation_proof_pending_carries_canonical_committee_dims() -> Result<()> {
    let (bus, rng, _seed, params, crp, _errors, history) =
        get_common_setup(Some(BfvPreset::InsecureThreshold512.into()))?;
    let e3_id = E3id::new("42", 1);
    let fhe = Arc::new(Fhe::new(params, crp, rng));
    let (initial_state, threshold_n, threshold_m, circuit_h) =
        verifying_c1_non_square_state(&fhe, &e3_id)?;

    let mut aggregator = PublicKeyAggregator::new(
        PublicKeyAggregatorParams {
            fhe,
            bus,
            e3_id: e3_id.clone(),
            params_preset: BfvPreset::InsecureThreshold512,
            committee_size: CiphernodesCommitteeSize::Micro,
        },
        test_state(initial_state),
    );

    let dishonest: BTreeSet<u64> = (circuit_h as u64..threshold_n as u64).collect();
    aggregator.handle_c1_verification_complete(TypedEvent::new(
        ShareVerificationComplete {
            e3_id: e3_id.clone(),
            kind: VerificationKind::PkGenerationProofs,
            dishonest_parties: dishonest,
        },
        test_ctx(ShareVerificationComplete {
            e3_id: e3_id.clone(),
            kind: VerificationKind::PkGenerationProofs,
            dishonest_parties: BTreeSet::new(),
        }),
    ))?;

    let event = next_event(&history).await?;
    assert!(matches!(
        event.into_data(),
        InterfoldEventData::PkAggregationProofPending(data)
            if data.e3_id == e3_id
                && data.proof_request.committee_n == threshold_n
                && data.proof_request.committee_h == circuit_h
                && data.proof_request.committee_threshold == threshold_m
                && data.proof_request.keyshare_bytes.len() == circuit_h
    ));

    Ok(())
}
