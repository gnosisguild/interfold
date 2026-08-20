// SPDX-License-Identifier: LGPL-3.0-only

//! Actix routing and lifecycle handlers for the registry writer.

use super::effects::*;
use super::*;
use e3_events::EventSource;

const PUBLICATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

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

impl<P: Provider + WalletProvider + Clone + 'static> CiphernodeRegistrySolWriter<P> {
    fn try_start_public_key(&mut self, e3_id: &E3id, ctx: &mut actix::Context<Self>) {
        if !self.request_registries.contains_key(e3_id) {
            return;
        }

        if let Some(intent) = self.publication.start(e3_id) {
            ctx.notify(SubmitPublicKey(intent));
        }
    }

    fn try_start_pending_public_keys(&mut self, ctx: &mut actix::Context<Self>) {
        for e3_id in self.publication.pending_keys() {
            self.try_start_public_key(&e3_id, ctx);
        }
    }
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
        let source = msg.source();
        match msg.into_data() {
            InterfoldEventData::EffectsEnabled(data) => self.notify_sync(ctx, data),
            InterfoldEventData::AggregatorChanged(data) => self.notify_sync(ctx, data),
            InterfoldEventData::DkgFoldAttestationContextEstablished(data) => {
                if self.provider.chain_id() == data.e3_id.chain_id() {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::PublicKeyAggregated(data) => {
                if source == EventSource::Local && self.provider.chain_id() == data.e3_id.chain_id()
                {
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

    fn handle(&mut self, _: EffectsEnabled, ctx: &mut Self::Context) -> Self::Result {
        self.effects_enabled = true;
        self.publication.enable_effects();
        self.try_start_pending_public_keys(ctx);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<AggregatorChanged>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: AggregatorChanged, ctx: &mut Self::Context) -> Self::Result {
        let e3_id = msg.e3_id;
        self.active_aggregators
            .insert(e3_id.clone(), msg.is_aggregator);
        if msg.is_aggregator {
            self.try_start_public_key(&e3_id, ctx);
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<DkgFoldAttestationContextEstablished>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(
        &mut self,
        msg: DkgFoldAttestationContextEstablished,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if !update_request_registry(&mut self.request_registries, &msg) {
            error!(
                e3_id = %msg.e3_id,
                schema_version = msg.schema_version,
                expected_schema_version = DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
                "Rejected DKG attestation context with an unsupported schema version"
            );
            return;
        }
        self.try_start_public_key(&msg.e3_id, ctx);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<E3RequestComplete>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: E3RequestComplete, _: &mut Self::Context) -> Self::Result {
        self.active_aggregators.remove(&msg.e3_id);
        if !self.publication.contains(&msg.e3_id) {
            self.request_registries.remove(&msg.e3_id);
        }
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
                let log_e3_id = msg.e3_id.clone();
                let contract_address = self.contract_address;
                let provider = self.provider.clone();
                let bus = self.bus.clone();

                Box::pin(async move {
                    info!("Submitting ticket {} for E3 {:?}", ticket_id, e3_id);

                    let result =
                        submit_ticket_to_registry(provider, contract_address, e3_id, ticket_id)
                            .await;
                    match result {
                        Ok(TxOutcome::Mined(receipt)) => {
                            info!(tx=%receipt.transaction_hash, "Ticket submitted to registry");
                        }
                        Ok(TxOutcome::AlreadySettled) => {
                            info!(e3_id = %log_e3_id, "Ticket already recorded on chain; skipping submission");
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

            let log_e3_id = e3_id.clone();
            let result = finalize_committee_on_registry(provider, contract_address, e3_id).await;
            match result {
                Ok(TxOutcome::Mined(receipt)) => {
                    info!(tx=%receipt.transaction_hash, "Committee finalized on registry");
                }
                Ok(TxOutcome::AlreadySettled) => {
                    info!(e3_id = %log_e3_id, "Committee finalized by another sender; nothing left to do");
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
    type Result = ();

    fn handle(&mut self, msg: PublicKeyAggregated, ctx: &mut Self::Context) -> Self::Result {
        let e3_id = msg.e3_id.clone();
        if self.effects_enabled && !self.is_active_aggregator_for(&e3_id) {
            info!(e3_id = %e3_id, "Ignoring public-key result while this node is not the active aggregator");
            return;
        }
        self.publication.record(e3_id.clone(), msg);
        self.try_start_public_key(&e3_id, ctx);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct SubmitPublicKey(PublicKeyAggregated);

impl<P: Provider + WalletProvider + Clone + 'static> Handler<SubmitPublicKey>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, command: SubmitPublicKey, _ctx: &mut Self::Context) -> Self::Result {
        let msg = command.0;
        if !self.publication.contains(&msg.e3_id) {
            self.publication.finish(&msg.e3_id, false);
            return Box::pin(async {}.into_actor(self));
        }
        let Some(contract_address) = self.request_registries.get(&msg.e3_id).copied() else {
            self.publication.finish(&msg.e3_id, false);
            return Box::pin(async {}.into_actor(self));
        };

        let e3_id = msg.e3_id.clone();
        let pubkey = msg.pubkey.clone();
        let pk_commitment = msg.pk_commitment;
        let dkg_aggregator_proof = msg.dkg_aggregator_proof.clone();
        let dkg_attestation_bundle = msg.dkg_attestation_bundle.clone();
        let provider = self.provider.clone();
        let bus = self.bus.clone();

        Box::pin(
            async move {
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
                        return (e3_id, false);
                    }
                    Ok(true) => true,
                };

                let result: Result<()> = async {
                    if should_publish {
                        let outcome = publish_committee_to_registry(
                            provider.clone(),
                            contract_address,
                            e3_id.clone(),
                            pk_commitment,
                            dkg_aggregator_proof.as_ref(),
                            dkg_attestation_bundle.as_ref().map(|b| b.as_ref()),
                        )
                        .await?;
                        match outcome.receipt() {
                            Some(receipt) => {
                                info!(tx=%receipt.transaction_hash, "Committee proof published to registry")
                            }
                            None => {
                                info!(e3_id = %e3_id, "Committee proof published by another aggregator; publishing the key candidate")
                            }
                        }
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

                match result {
                    Ok(()) => (e3_id, true),
                    Err(err) => {
                        error!(
                            "Failed to publish committee data: {}",
                            format_evm_error(&err)
                        );
                        bus.err(EType::Evm, err);
                        (e3_id, false)
                    }
                }
            }
            .into_actor(self)
            .map(|(e3_id, terminal), actor, ctx| {
                actor.publication.finish(&e3_id, terminal);
                if terminal {
                    actor.request_registries.remove(&e3_id);
                } else {
                    ctx.run_later(PUBLICATION_RETRY_DELAY, move |actor, ctx| {
                        actor.try_start_public_key(&e3_id, ctx);
                    });
                }
            }),
        )
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
