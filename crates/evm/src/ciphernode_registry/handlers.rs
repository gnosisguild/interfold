// SPDX-License-Identifier: LGPL-3.0-only

//! Actix routing and lifecycle handlers for the registry writer.

use super::effects::*;
use super::*;

fn update_request_registry(
    registries: &mut HashMap<E3id, Address>,
    event: &DkgFoldAttestationContextEstablished,
) -> bool {
    if event.schema_version != DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION {
        registries.remove(&event.e3_id);
        return false;
    }

    registries.insert(event.e3_id.clone(), event.context.registry);
    true
}

impl<P: Provider + WalletProvider + Clone + 'static> Actor for CiphernodeRegistrySolWriter<P> {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<InterfoldEvent>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        match msg.into_data() {
            InterfoldEventData::EffectsEnabled(data) => self.notify_sync(ctx, data),
            InterfoldEventData::AggregatorChanged(data) => self.notify_sync(ctx, data),
            InterfoldEventData::DkgFoldAttestationContextEstablished(data) => {
                if self.provider.chain_id() == data.e3_id.chain_id() {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::PublicKeyAggregated(data) => {
                // Only publish if the src and destination chains match
                if self.provider.chain_id() == data.e3_id.chain_id() {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::CommitteeFinalizeRequested(data) => {
                if self.provider.chain_id() == data.e3_id.chain_id() {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::TicketGenerated(data) => {
                // Submit ticket if chain matches
                if self.provider.chain_id() == data.e3_id.chain_id() {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::E3RequestComplete(data) => self.notify_sync(ctx, data),
            InterfoldEventData::Shutdown(data) => self.notify_sync(ctx, data),
            _ => (),
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<EffectsEnabled>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, _: EffectsEnabled, _: &mut Self::Context) -> Self::Result {
        self.effects_enabled = true;
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<AggregatorChanged>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: AggregatorChanged, _: &mut Self::Context) -> Self::Result {
        self.active_aggregators.insert(msg.e3_id, msg.is_aggregator);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<DkgFoldAttestationContextEstablished>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(
        &mut self,
        msg: DkgFoldAttestationContextEstablished,
        _: &mut Self::Context,
    ) -> Self::Result {
        if !update_request_registry(&mut self.request_registries, &msg) {
            error!(
                e3_id = %msg.e3_id,
                schema_version = msg.schema_version,
                expected_schema_version = DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
                "Rejected DKG attestation context with an unsupported schema version"
            );
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<E3RequestComplete>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: E3RequestComplete, _: &mut Self::Context) -> Self::Result {
        self.active_aggregators.remove(&msg.e3_id);
        self.request_registries.remove(&msg.e3_id);
        self.submitting.remove(&msg.e3_id);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<TicketGenerated>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: TicketGenerated, _: &mut Self::Context) -> Self::Result {
        if !self.effects_enabled {
            return Box::pin(async {});
        }

        match msg.ticket_id {
            TicketId::Score(ticket_id) => {
                info!(
                    "Score sortition ticket generated for E3 {:?}, submitting to contract",
                    msg.e3_id
                );

                let e3_id = msg.e3_id.clone();
                let contract_address = self.contract_address;
                let provider = self.provider.clone();
                let bus = self.bus.clone();

                Box::pin(async move {
                    info!("Submitting ticket {} for E3 {:?}", ticket_id, e3_id);

                    let result =
                        submit_ticket_to_registry(provider, contract_address, e3_id, ticket_id)
                            .await;
                    match result {
                        Ok(receipt) => {
                            info!(tx=%receipt.transaction_hash, "Ticket submitted to registry");
                        }
                        Err(err) => {
                            error!("Failed to submit ticket: {}", format_evm_error(&err));
                            bus.err(EType::Evm, err);
                        }
                    }
                })
            }
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<CommitteeFinalizeRequested>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: CommitteeFinalizeRequested, _: &mut Self::Context) -> Self::Result {
        if !self.effects_enabled {
            return Box::pin(async {});
        }

        let e3_id = msg.e3_id.clone();
        let contract_address = self.contract_address;
        let provider = self.provider.clone();
        let bus = self.bus.clone();

        Box::pin(async move {
            match should_finalize_committee(provider.clone(), contract_address, e3_id.clone()).await
            {
                Ok(false) => {
                    info!(e3_id = %e3_id, "Skipping finalizeCommittee; on-chain state is not finalizable");
                    return;
                }
                Err(err) => {
                    error!(
                        "Failed to preflight finalizeCommittee: {}",
                        format_evm_error(&err)
                    );
                    return;
                }
                Ok(true) => {}
            }

            info!("Finalizing committee for E3 {:?}", e3_id);

            let result = finalize_committee_on_registry(provider, contract_address, e3_id).await;
            match result {
                Ok(receipt) => {
                    info!(tx=%receipt.transaction_hash, "Committee finalized on registry");
                }
                Err(err) => {
                    error!("Failed to finalize committee: {}", format_evm_error(&err));
                    bus.err(EType::Evm, err);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::update_request_registry;
    use alloy::primitives::Address;
    use e3_events::{
        DkgFoldAttestationContext, DkgFoldAttestationContextEstablished, E3id,
        DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
    };
    use std::collections::HashMap;

    #[test]
    fn valid_context_records_the_request_time_registry() {
        let e3_id = E3id::new("7", 1);
        let registry = Address::repeat_byte(0x11);
        let event = DkgFoldAttestationContextEstablished {
            schema_version: DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
            e3_id: e3_id.clone(),
            context: DkgFoldAttestationContext {
                registry,
                verifying_contract: Address::repeat_byte(0x22),
            },
        };
        let mut registries = HashMap::new();

        assert!(update_request_registry(&mut registries, &event));
        assert_eq!(registries.get(&e3_id), Some(&registry));
    }

    #[test]
    fn unsupported_context_removes_the_cached_registry() {
        let e3_id = E3id::new("8", 1);
        let registry = Address::repeat_byte(0x33);
        let event = DkgFoldAttestationContextEstablished {
            schema_version: DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION + 1,
            e3_id: e3_id.clone(),
            context: DkgFoldAttestationContext {
                registry,
                verifying_contract: Address::repeat_byte(0x44),
            },
        };
        let mut registries = HashMap::from([(e3_id.clone(), registry)]);

        assert!(!update_request_registry(&mut registries, &event));
        assert!(!registries.contains_key(&e3_id));
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<PublicKeyAggregated>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: PublicKeyAggregated, ctx: &mut Self::Context) -> Self::Result {
        if !self.effects_enabled || !self.is_active_aggregator_for(&msg.e3_id) {
            return Box::pin(async {});
        }

        let Some(contract_address) = self.request_registries.get(&msg.e3_id).copied() else {
            error!(
                e3_id = %msg.e3_id,
                "Cannot publish a committee without its request-time registry"
            );
            return Box::pin(async {});
        };

        // Do not submit a second transaction while publishCommittee is in progress
        // for this E3. The on-chain preflight prevents duplicates after a restart.
        if !self.submitting.insert(msg.e3_id.clone()) {
            info!(e3_id = %msg.e3_id, "publishCommittee already in flight; skipping duplicate submission");
            return Box::pin(async {});
        }

        let e3_id = msg.e3_id.clone();
        let pubkey = msg.pubkey.clone();
        let pk_commitment = msg.pk_commitment;
        let dkg_aggregator_proof = msg.dkg_aggregator_proof.clone();
        let dkg_attestation_bundle = msg.dkg_attestation_bundle.clone();
        let provider = self.provider.clone();
        let bus = self.bus.clone();
        let self_addr = ctx.address();

        Box::pin(async move {
            let should_publish = match should_publish_committee(
                provider.clone(),
                contract_address,
                e3_id.clone(),
                pk_commitment,
            )
            .await
            {
                Ok(false) => {
                    info!(e3_id = %e3_id, "Committee proof already published; publishing the key candidate");
                    false
                }
                Err(err) => {
                    error!(
                        "Failed to preflight publishCommittee: {}",
                        format_evm_error(&err)
                    );
                    // Transient read failure: allow a later event to retry.
                    self_addr.do_send(ClearSubmitting(e3_id));
                    return;
                }
                Ok(true) => true,
            };

            let result: Result<()> = async {
                if should_publish {
                    let receipt = publish_committee_to_registry(
                        provider.clone(),
                        contract_address,
                        e3_id.clone(),
                        pk_commitment,
                        dkg_aggregator_proof.as_ref(),
                        dkg_attestation_bundle.as_ref().map(|b| b.as_ref()),
                    )
                    .await?;
                    info!(tx=%receipt.transaction_hash, "Committee proof published to registry");
                }

                let receipt = publish_committee_public_key_to_registry(
                    provider,
                    contract_address,
                    e3_id.clone(),
                    pubkey,
                )
                .await?;
                info!(tx=%receipt.transaction_hash, "Committee public-key candidate published to registry");
                Ok(())
            }
            .await;

            if let Err(err) = result {
                error!(
                    "Failed to publish committee data: {}",
                    format_evm_error(&err)
                );
                self_addr.do_send(ClearSubmitting(e3_id));
                bus.err(EType::Evm, err);
            }
        })
    }
}

/// Internal message: clear the in-flight `publishCommittee` marker for an E3 so
/// a subsequent submission attempt is allowed after a failure (H13).
#[derive(Message)]
#[rtype(result = "()")]
struct ClearSubmitting(E3id);

impl<P: Provider + WalletProvider + Clone + 'static> Handler<ClearSubmitting>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: ClearSubmitting, _: &mut Self::Context) -> Self::Result {
        self.submitting.remove(&msg.0);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<Shutdown>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, _: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}
