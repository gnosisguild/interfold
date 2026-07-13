// SPDX-License-Identifier: LGPL-3.0-only

//! Decryption-key calculation and early C4 share handling.

use super::*;

impl ThresholdKeyshare {
    /// After verification, decrypt shares from honest parties and compute the decryption key.
    /// C4 proof generation is deferred to ProofRequestActor via DecryptionShareProofsPending.
    pub(in crate::actors::threshold_keyshare) fn proceed_with_decryption_key_calculation(
        &mut self,
        dishonest_parties: Option<HashSet<u64>>,
        ec: EventContext<Sequenced>,
    ) -> Result<()> {
        let state = self.state.try_get()?;
        let e3_id = state.get_e3_id();
        let trbfv_config = state.get_trbfv_config();

        // Get our BFV secret key from state, pending shares from the actor
        let current: AggregatingDecryptionKey = state.clone().try_into()?;
        let shares = std::mem::take(&mut self.pending.shares);

        let plan = build_decryption_key_plan(
            &self.cipher,
            self.share_enc_preset,
            state.party_id,
            state.threshold_m,
            state.threshold_n,
            trbfv_config,
            &current,
            shares,
            dishonest_parties,
            e3_id,
        )?;

        match plan {
            DecryptionKeyPlan::Insufficient => {
                self.pending.shares.clear();
                self.bus.publish(
                    E3Failed {
                        e3_id: e3_id.clone(),
                        failed_at_stage: E3Stage::CommitteeFinalized,
                        reason: FailureReason::InsufficientCommitteeMembers,
                    },
                    ec,
                )?;
            }
            DecryptionKeyPlan::Proceed {
                calc_request,
                sk_request,
                esm_requests,
                honest_party_ids,
            } => {
                // Publish CalculateDecryptionKey request before persisting (ordering preserved).
                let event = ComputeRequest::trbfv(
                    TrBFVRequest::CalculateDecryptionKey(calc_request),
                    CorrelationId::new(),
                    e3_id.clone(),
                );
                self.bus.publish(event, ec.clone())?;

                // Store honest parties and C4 data on the actor (transient coordination)
                self.state.try_mutate(&ec, |mut s| {
                    s.honest_parties = Some(honest_party_ids.clone());
                    Ok(s)
                })?;
                self.pending.share_decryption_data = Some((sk_request, esm_requests));
            }
        }

        Ok(())
    }

    /// 5a. CalculateDecryptionKeyResponse — transition to ReadyForDecryption,
    /// then publish DecryptionShareProofsPending so ProofRequestActor can
    /// generate C4 proofs, sign them, and publish DecryptionKeyShared.
    pub fn handle_calculate_decryption_key_response(
        &mut self,
        res: TypedEvent<ComputeResponse>,
        self_addr: Addr<Self>,
    ) -> Result<()> {
        let (res, ec) = res.into_components();
        let output: CalculateDecryptionKeyResponse = res
            .try_into()
            .context("Error extracting data from compute process")?;

        let (sk_poly_sum, es_poly_sum) = (output.sk_poly_sum, output.es_poly_sum);

        // Extract C4 data from the actor (stored by proceed_with_decryption_key_calculation)
        let (sk_request, esm_requests) = self
            .pending
            .share_decryption_data
            .take()
            .ok_or_else(|| anyhow!("No pending share decryption data — CalculateDecryptionKey responded before proof requests were built"))?;

        // Take early shares from the actor before transitioning
        let early_shares = self
            .pending
            .c4_verification_shares
            .take()
            .unwrap_or_default();

        // Transition to ReadyForDecryption
        self.state.try_mutate(&ec, |s| {
            use KeyshareState as K;
            info!("Try store decryption key");

            let current: AggregatingDecryptionKey = s.clone().try_into()?;

            let next = K::ReadyForDecryption(ReadyForDecryption {
                pk_share: current.pk_share,
                sk_poly_sum,
                es_poly_sum,
                signed_pk_generation_proof: current.signed_pk_generation_proof,
                signed_sk_share_computation_proof: current.signed_sk_share_computation_proof,
                signed_e_sm_share_computation_proof: current.signed_e_sm_share_computation_proof,
                signed_sk_share_encryption_proofs: current.signed_sk_share_encryption_proofs,
                signed_e_sm_share_encryption_proofs: current.signed_e_sm_share_encryption_proofs,
            });

            s.new_state(next)
        })?;

        // Publish DecryptionShareProofsPending to ProofRequestActor
        let state = self.state.try_get()?;
        let e3_id = state.get_e3_id();
        let party_id = state.party_id;
        let node = state.address.clone();

        info!(
            "Publishing DecryptionShareProofsPending for E3 {} party {} (1 SK + {} ESM requests)",
            e3_id,
            party_id,
            esm_requests.len()
        );

        self.bus.publish(
            DecryptionShareProofsPending {
                e3_id: e3_id.clone(),
                party_id,
                node,
                sk_request,
                esm_requests,
            },
            ec.clone(),
        )?;

        // Create collector and replay any early-arriving DecryptionKeyShared events
        let state = self.state.try_get()?;
        let my_party_id = state.party_id;
        let honest = state.honest_parties.as_ref().cloned().unwrap_or_default();
        let expected: HashSet<u64> = honest
            .iter()
            .filter(|&&pid| pid != my_party_id)
            .copied()
            .collect();

        if !expected.is_empty() {
            let collector = self.ensure_decryption_key_shared_collector(self_addr)?;
            for (_pid, share) in early_shares {
                collector.do_send(TypedEvent::new(share, ec.clone()));
            }
        }

        Ok(())
    }

    /// Handle an external DecryptionKeyShared event while in AggregatingDecryptionKey state.
    /// Store it for later processing when we transition to ReadyForDecryption.
    pub(in crate::actors::threshold_keyshare) fn handle_early_decryption_key_share(
        &mut self,
        data: DecryptionKeyShared,
        _ec: EventContext<Sequenced>,
    ) -> Result<()> {
        let party_id = data.party_id;
        let state = self.state.try_get()?;
        if state.expelled_parties.contains(&party_id) {
            info!(
                "Dropping early DecryptionKeyShared from expelled party {}",
                party_id
            );
            return Ok(());
        }
        info!(
            "Storing early DecryptionKeyShared from party {} (state: AggregatingDecryptionKey)",
            party_id
        );
        self.pending
            .c4_verification_shares
            .get_or_insert_with(HashMap::new)
            .insert(party_id, data);
        Ok(())
    }
}
