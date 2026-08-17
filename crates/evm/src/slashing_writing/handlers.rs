// SPDX-License-Identifier: LGPL-3.0-only

//! Admission, scheduling, and submission-outcome handlers.

use super::effects::{read_slash_policy, submit_slash_proposal};
use super::*;

impl<P: Provider + WalletProvider + Clone + 'static> Handler<InterfoldEvent>
    for SlashingManagerSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        match msg.into_data() {
            InterfoldEventData::AccusationQuorumReached(data) => {
                // Every node evaluates the policy after quorum. Only the first three voters send
                // a transaction when Lane A is enabled, but all nodes need the same disabled-policy
                // decision so they can derive the same E3-scoped exclusion.
                if self.provider.chain_id() == data.e3_id.chain_id()
                    && is_slashable_outcome(&data.outcome)
                {
                    match self.submissions.admit(data.clone()) {
                        Ok((key, SlashSubmissionDecision::Submit)) => {
                            ctx.notify(SubmitSlashIntent { key, event: data });
                        }
                        Ok((_, SlashSubmissionDecision::Defer)) => {
                            info!(e3_id = %data.e3_id, "Deferred slash intent until effects are enabled");
                        }
                        Ok((_, SlashSubmissionDecision::IgnoreDuplicate)) => {
                            info!(e3_id = %data.e3_id, "Ignored duplicate slash intent");
                        }
                        Err(error) => self.bus.err(EType::Evm, error),
                    }
                }
            }
            InterfoldEventData::CommitteeMemberExcluded(data) => {
                if data.e3_id.chain_id() == self.provider.chain_id() {
                    match SlashIntentKey::from_exclusion(&data) {
                        Ok(key) => self.submissions.mark_completed(key),
                        Err(error) => self.bus.err(EType::Evm, error),
                    }
                }
            }
            InterfoldEventData::EffectsEnabled(_) => {
                let deferred = self.submissions.enable_effects();
                if !deferred.is_empty() {
                    info!(
                        intents = deferred.len(),
                        "Releasing deferred slash intents after startup reconciliation"
                    );
                    let address = ctx.address();
                    ctx.spawn(
                        async move {
                            for (key, event) in deferred {
                                if let Err(error) =
                                    address.send(SubmitSlashIntent { key, event }).await
                                {
                                    warn!(%error, "Slashing writer stopped with deferred intents pending");
                                    break;
                                }
                            }
                        }
                        .into_actor(self),
                    );
                }
            }
            InterfoldEventData::Shutdown(data) => self.notify_sync(ctx, data),
            _ => (),
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<SubmitSlashIntent>
    for SlashingManagerSolWriter<P>
{
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: SubmitSlashIntent, ctx: &mut Self::Context) -> Self::Result {
        Box::pin({
            let contract_address = self.contract_address;
            let provider = self.provider.clone();
            let bus = self.bus.clone();
            let my_addr = self.provider.provider().default_signer_address();
            let address = ctx.address();
            async move {
                let SubmitSlashIntent { key, event: msg } = msg;
                let rank = submission_rank(msg.votes_for.iter().map(|v| v.voter), my_addr);

                let policy =
                    read_slash_policy(provider.clone(), contract_address, msg.proof_type as u8)
                        .await;

                match policy {
                    Ok(policy)
                        if classify_slash_policy(policy.enabled, policy.requiresProof)
                            == SlashPolicyState::Disabled =>
                    {
                        info!(
                            e3_id = %msg.e3_id,
                            accused = %msg.accused,
                            proof_type = %msg.proof_type,
                            reason = %slash_reason(msg.proof_type as u8),
                            "Slash policy is disabled; excluding the faulted member from local E3 work"
                        );
                        if let Err(error) = bus.publish_without_context(CommitteeMemberExcluded {
                            e3_id: msg.e3_id.clone(),
                            node: msg.accused,
                            proof_type: msg.proof_type,
                            party_id: None,
                        }) {
                            bus.err(EType::Evm, error);
                            let _ = address
                                .send(SlashSubmissionFinished {
                                    key,
                                    terminal: false,
                                })
                                .await;
                            return;
                        }
                        let _ = address
                            .send(SlashSubmissionFinished {
                                key,
                                terminal: true,
                            })
                            .await;
                        return;
                    }
                    Ok(policy)
                        if classify_slash_policy(policy.enabled, policy.requiresProof)
                            == SlashPolicyState::InvalidForAttestations =>
                    {
                        bus.err(
                            EType::Evm,
                            anyhow::anyhow!(
                                "Slash policy for proof type {} is enabled but does not accept committee attestations",
                                msg.proof_type
                            ),
                        );
                        let _ = address
                            .send(SlashSubmissionFinished {
                                key,
                                terminal: true,
                            })
                            .await;
                        return;
                    }
                    Err(error) => {
                        // A failed read must not invent an exclusion. Eligible submitters retain the
                        // previous transaction path so a temporary RPC read failure cannot suppress
                        // an enabled slash.
                        warn!(%error, "Could not read slash policy before submission");
                    }
                    Ok(_) => {}
                }

                if !should_submit_slash(true, &msg.outcome, rank) {
                    let _ = address
                        .send(SlashSubmissionFinished {
                            key,
                            terminal: true,
                        })
                        .await;
                    return;
                }

                if encode_attestation_evidence(&msg).is_none() {
                    bus.err(
                        EType::Evm,
                        anyhow::anyhow!(
                            "Refusing malformed slash intent for E3 {}: votes or evidence are empty",
                            msg.e3_id
                        ),
                    );
                    let _ = address
                        .send(SlashSubmissionFinished {
                            key,
                            terminal: true,
                        })
                        .await;
                    return;
                }

                let rank = rank.expect("submission decision requires a voter rank");

                // Fallback submitters wait before attempting, giving the primary
                // submitter time to land the transaction on-chain.
                if rank > 0 {
                    let delay = submission_delay(rank);
                    info!(
                        "Fallback submitter (rank {rank}): waiting {delay:?} before submission attempt"
                    );
                    tokio::time::sleep(delay).await;
                }

                let result = submit_slash_proposal(provider, contract_address, msg).await;
                let terminal = match result {
                    Ok(receipt) => {
                        info!(tx=%receipt.transaction_hash, "Submitted attestation-based slash proposal on-chain");
                        true
                    }
                    Err(err) => {
                        let decoded = format_evm_error(&err);
                        let benign = decoded.contains("OperatorNotInCommittee")
                            || decoded.contains("VoterNotInCommittee")
                            || decoded.contains("DuplicateEvidence");
                        if benign {
                            // Fallback submitters expect DuplicateEvidence reverts
                            // when the primary submitter has already landed the tx.
                            // Operator/VoterNotInCommittee indicate a stale off-chain accusation
                            // (e.g. cross-E3 race) — not a node-local fault.
                            warn!("Slash submission skipped (rank {rank}): {decoded}");
                        } else {
                            bus.err(
                                EType::Evm,
                                anyhow::anyhow!("Error submitting slash proposal: {decoded}"),
                            );
                        }
                        benign
                    }
                };
                if let Err(error) = address
                    .send(SlashSubmissionFinished { key, terminal })
                    .await
                {
                    warn!(%error, "Slashing writer stopped before recording submission outcome");
                }
            }
        })
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<SlashSubmissionFinished>
    for SlashingManagerSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: SlashSubmissionFinished, _: &mut Self::Context) -> Self::Result {
        self.submissions.finish(&msg.key, msg.terminal);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<Shutdown>
    for SlashingManagerSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, _: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}
