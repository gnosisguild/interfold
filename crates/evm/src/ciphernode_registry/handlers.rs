// SPDX-License-Identifier: LGPL-3.0-only

//! Actix routing, durable effect admission, and replay for the registry writer.

use super::effects::*;
use super::*;
use crate::{reconcile_dispatched, DispatchReconciliation, OutboxAdmission};

#[derive(Message)]
#[rtype(result = "()")]
struct RegistryEffectFinished(String);

impl<P: Provider + WalletProvider + Clone + 'static> CiphernodeRegistrySolWriter<P> {
    fn admit_effect(&mut self, effect: RegistryEffect, ctx: &mut Context<Self>) {
        let key = effect.key();
        let outbox = self.outbox.clone();
        let bus = self.bus.clone();
        ctx.wait(
            async move { outbox.admit(key, effect).await }
                .into_actor(self)
                .map(move |result, _, ctx| match result {
                    Ok(OutboxAdmission::AlreadyTerminal) => {}
                    Ok(OutboxAdmission::Inserted | OutboxAdmission::AlreadyPending) => {
                        ctx.notify(DrainRegistryOutbox);
                    }
                    Err(error) => bus.err(EType::Evm, error),
                }),
        );
    }

    fn can_execute(&self, effect: &RegistryEffect) -> bool {
        self.effects_enabled
            && match effect {
                RegistryEffect::PublishCommitteeProof { event, .. }
                | RegistryEffect::PublishCommitteePublicKey { event, .. } => {
                    self.is_active_aggregator_for(&event.e3_id)
                }
                RegistryEffect::SubmitTicket(_) | RegistryEffect::FinalizeCommittee(_) => true,
            }
    }
}

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
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
        ctx.run_interval(Duration::from_secs(30), |actor, ctx| {
            if actor.effects_enabled {
                ctx.notify(DrainRegistryOutbox);
            }
        });
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
                if self.provider.chain_id() == data.e3_id.chain_id() {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::CommitteeFinalizeRequested(data)
                if self.provider.chain_id() == data.e3_id.chain_id() =>
            {
                self.admit_effect(RegistryEffect::FinalizeCommittee(data), ctx)
            }
            InterfoldEventData::TicketGenerated(data)
                if self.provider.chain_id() == data.e3_id.chain_id() =>
            {
                self.admit_effect(RegistryEffect::SubmitTicket(data), ctx)
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
        ctx.notify(DrainRegistryOutbox);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<AggregatorChanged>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: AggregatorChanged, ctx: &mut Self::Context) -> Self::Result {
        let became_active = msg.is_aggregator;
        self.active_aggregators.insert(msg.e3_id, msg.is_aggregator);
        if became_active && self.effects_enabled {
            ctx.notify(DrainRegistryOutbox);
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
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<TicketGenerated>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: TicketGenerated, ctx: &mut Self::Context) -> Self::Result {
        self.admit_effect(RegistryEffect::SubmitTicket(msg), ctx);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<CommitteeFinalizeRequested>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: CommitteeFinalizeRequested, ctx: &mut Self::Context) -> Self::Result {
        self.admit_effect(RegistryEffect::FinalizeCommittee(msg), ctx);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<PublicKeyAggregated>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: PublicKeyAggregated, ctx: &mut Self::Context) -> Self::Result {
        let Some(registry) = self.request_registries.get(&msg.e3_id).copied() else {
            error!(
                e3_id = %msg.e3_id,
                "Cannot publish a committee without its request-time registry"
            );
            return;
        };

        self.admit_effect(
            RegistryEffect::PublishCommitteeProof {
                registry,
                event: msg.clone(),
            },
            ctx,
        );
        self.admit_effect(
            RegistryEffect::PublishCommitteePublicKey {
                registry,
                event: msg,
            },
            ctx,
        );
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<DrainRegistryOutbox>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, _: DrainRegistryOutbox, ctx: &mut Self::Context) -> Self::Result {
        if !self.effects_enabled {
            return;
        }
        let outbox = self.outbox.clone();
        ctx.wait(async move { outbox.pending().await }.into_actor(self).map(
            |pending, actor, ctx| {
                for (key, effect, status) in pending {
                    if actor.can_execute(&effect) && actor.submitting.insert(key.clone()) {
                        ctx.notify(ExecuteRegistryEffect {
                            key,
                            effect,
                            status,
                        });
                    }
                }
            },
        ));
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<ExecuteRegistryEffect>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: ExecuteRegistryEffect, ctx: &mut Self::Context) -> Self::Result {
        let provider = self.provider.clone();
        let contract_address = self.contract_address;
        let outbox = self.outbox.clone();
        let bus = self.bus.clone();
        let address = ctx.address();

        Box::pin(async move {
            let ExecuteRegistryEffect {
                key,
                effect,
                status,
            } = msg;
            let result: Result<()> = async {
                match reconcile_dispatched(&provider, &outbox, &key, &status).await? {
                    DispatchReconciliation::Pending | DispatchReconciliation::Terminal => {
                        return Ok(())
                    }
                    DispatchReconciliation::NotDispatched | DispatchReconciliation::Retry => {}
                }

                match effect {
                    RegistryEffect::SubmitTicket(event) => {
                        let TicketId::Score(ticket_id) = event.ticket_id;
                        if !should_submit_ticket(
                            provider.clone(),
                            contract_address,
                            event.e3_id.clone(),
                            ticket_id,
                        )
                        .await?
                        {
                            outbox.mark_terminal(&key).await?;
                            return Ok(());
                        }
                        let receipt = submit_ticket_to_registry(
                            provider.clone(),
                            contract_address,
                            event.e3_id,
                            ticket_id,
                            &outbox,
                            &key,
                        )
                        .await?;
                        info!(tx=%receipt.transaction_hash, "Ticket submitted to registry");
                    }
                    RegistryEffect::FinalizeCommittee(event) => {
                        match should_finalize_committee(
                            provider.clone(),
                            contract_address,
                            event.e3_id.clone(),
                        )
                        .await?
                        {
                            FinalizeCommitteePreflight::Terminal => {
                                outbox.mark_terminal(&key).await?;
                                return Ok(());
                            }
                            FinalizeCommitteePreflight::Retry => return Ok(()),
                            FinalizeCommitteePreflight::Submit => {}
                        }
                        let receipt = finalize_committee_on_registry(
                            provider.clone(),
                            contract_address,
                            event.e3_id,
                            &outbox,
                            &key,
                        )
                        .await?;
                        info!(tx=%receipt.transaction_hash, "Committee finalized on registry");
                    }
                    RegistryEffect::PublishCommitteeProof { registry, event } => {
                        if !should_publish_committee(
                            provider.clone(),
                            registry,
                            event.e3_id.clone(),
                            event.pk_commitment,
                        )
                        .await?
                        {
                            outbox.mark_terminal(&key).await?;
                            return Ok(());
                        }
                        let receipt = publish_committee_to_registry(
                            provider.clone(),
                            registry,
                            event,
                            &outbox,
                            &key,
                        )
                        .await?;
                        info!(tx=%receipt.transaction_hash, "Committee proof published to registry");
                    }
                    RegistryEffect::PublishCommitteePublicKey { registry, event } => {
                        if should_publish_committee(
                            provider.clone(),
                            registry,
                            event.e3_id.clone(),
                            event.pk_commitment,
                        )
                        .await?
                        {
                            return Ok(());
                        }
                        let receipt = publish_committee_public_key_to_registry(
                            provider.clone(),
                            registry,
                            event,
                            &outbox,
                            &key,
                        )
                        .await?;
                        info!(tx=%receipt.transaction_hash, "Committee public-key candidate published to registry");
                    }
                }
                outbox.mark_terminal(&key).await?;
                Ok(())
            }
            .await;

            if let Err(error) = result {
                error!(effect_key = %key, "Durable registry effect remains pending: {}", format_evm_error(&error));
                bus.err(EType::Evm, error);
            }
            if let Err(error) = address.send(RegistryEffectFinished(key)).await {
                error!(%error, "Registry writer stopped before clearing in-flight effect");
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

impl<P: Provider + WalletProvider + Clone + 'static> Handler<RegistryEffectFinished>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: RegistryEffectFinished, _: &mut Self::Context) -> Self::Result {
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
