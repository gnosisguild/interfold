// SPDX-License-Identifier: LGPL-3.0-only

//! Message routing and actor lifecycle.

use super::effects::*;
use super::*;
use e3_events::EventSource;

const PUBLICATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

impl<P: Provider + WalletProvider + Clone + 'static> InterfoldSolWriter<P> {
    fn try_start_plaintext(&mut self, e3_id: &E3id, ctx: &mut actix::Context<Self>) {
        if !self.is_active_aggregator_for(e3_id) {
            return;
        }
        if let Some(intent) = self.publication.start(e3_id) {
            ctx.notify(SubmitPlaintext(intent));
        }
    }

    fn try_start_pending_plaintexts(&mut self, ctx: &mut actix::Context<Self>) {
        for e3_id in self.publication.pending_keys() {
            self.try_start_plaintext(&e3_id, ctx);
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<InterfoldEvent>
    for InterfoldSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, ctx: &mut Self::Context) -> Self::Result {
        let source = msg.source();
        match msg.into_data() {
            InterfoldEventData::EffectsEnabled(data) => self.notify_sync(ctx, data),
            InterfoldEventData::AggregatorChanged(data) => self.notify_sync(ctx, data),
            InterfoldEventData::PlaintextAggregated(data) => {
                // Only a locally computed result is a publication intent. Peer results are
                // inputs for protocol observers and must not cross the EVM write boundary.
                if source == EventSource::Local && self.provider.chain_id() == data.e3_id.chain_id()
                {
                    ctx.notify(data);
                }
            }
            InterfoldEventData::E3StageChanged(data) => {
                // When an E3 transitions to Failed on-chain, call processE3Failure
                // to finalize refund distribution automatically.
                if data.new_stage == E3Stage::Failed
                    && self.provider.chain_id() == data.e3_id.chain_id()
                {
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
    for InterfoldSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, _: EffectsEnabled, ctx: &mut Self::Context) -> Self::Result {
        self.effects_enabled = true;
        self.publication.enable_effects();
        self.try_start_pending_plaintexts(ctx);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<AggregatorChanged>
    for InterfoldSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: AggregatorChanged, ctx: &mut Self::Context) -> Self::Result {
        let e3_id = msg.e3_id;
        self.active_aggregators
            .insert(e3_id.clone(), msg.is_aggregator);
        if msg.is_aggregator {
            self.try_start_plaintext(&e3_id, ctx);
        }
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<E3RequestComplete>
    for InterfoldSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: E3RequestComplete, _: &mut Self::Context) -> Self::Result {
        self.active_aggregators.remove(&msg.e3_id);
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<PlaintextAggregated>
    for InterfoldSolWriter<P>
{
    type Result = ();

    fn handle(&mut self, msg: PlaintextAggregated, ctx: &mut Self::Context) -> Self::Result {
        let e3_id = msg.e3_id.clone();
        // Replay retains the durable local intent while the persisted aggregator role is restored.
        // Live results still require the active role when they enter the outbox, and every
        // submission attempt is role-gated by `try_start_plaintext`.
        if self.effects_enabled && !self.is_active_aggregator_for(&e3_id) {
            info!(e3_id = %e3_id, "Ignoring plaintext result while this node is not the active aggregator");
            return;
        }
        self.publication.record(e3_id.clone(), msg);
        self.try_start_plaintext(&e3_id, ctx);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct SubmitPlaintext(PlaintextAggregated);

impl<P: Provider + WalletProvider + Clone + 'static> Handler<SubmitPlaintext>
    for InterfoldSolWriter<P>
{
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, command: SubmitPlaintext, _ctx: &mut Self::Context) -> Self::Result {
        let msg = command.0;
        if !self.is_active_aggregator_for(&msg.e3_id) || !self.publication.contains(&msg.e3_id) {
            self.publication.finish(&msg.e3_id, false);
            return Box::pin(async {}.into_actor(self));
        }

        Box::pin(
            {
            let e3_id = msg.e3_id.clone();
            let decrypted_output = msg.decrypted_output.clone();
            let contract_address = self.contract_address;
            let provider = self.provider.clone();
            let bus = self.bus.clone();
            async move {
                // The event can represent multiple ciphertext outputs, but the contract accepts one
                // plaintext output per E3. Validation rejects multi-output results before indexing.
                if let Err(msg_err) = validate_plaintext_output(
                    &e3_id,
                    &decrypted_output,
                    &msg.decryption_aggregator_proofs,
                ) {
                    bus.err(EType::Evm, anyhow::anyhow!(msg_err));
                    return (e3_id, true);
                }
                // Safe: `validate_plaintext_output` guarantees exactly one output.
                let decrypted = &decrypted_output[0];
                match should_publish_plaintext(provider.clone(), contract_address, e3_id.clone())
                    .await
                {
                    Ok(false) => {
                        info!(e3_id = %e3_id, "Skipping publishPlaintextOutput; plaintext already published");
                        return (e3_id, true);
                    }
                    Err(err) => {
                        bus.err(
                            EType::Evm,
                            anyhow::anyhow!(
                                "Error preflighting plaintext publication: {}",
                                format_evm_error(&err)
                            ),
                        );
                        return (e3_id, false);
                    }
                    Ok(true) => {}
                }

                let result = publish_plaintext_output(
                    provider,
                    contract_address,
                    e3_id.clone(),
                    decrypted.extract_bytes(),
                    msg.decryption_aggregator_proofs.first(),
                )
                .await;
                match result {
                    Ok(receipt) => {
                        info!(tx=%receipt.transaction_hash, "Published plaintext output");
                        (e3_id, true)
                    }
                    Err(err) => {
                        bus.err(
                            EType::Evm,
                            anyhow::anyhow!(
                                "Error publishing plaintext output: {}",
                                format_evm_error(&err)
                            ),
                        );
                        (e3_id, false)
                    }
                }
            }
        }
            .into_actor(self)
            .map(|(e3_id, terminal), actor, ctx| {
                actor.publication.finish(&e3_id, terminal);
                if !terminal {
                    ctx.run_later(PUBLICATION_RETRY_DELAY, move |actor, ctx| {
                        actor.try_start_plaintext(&e3_id, ctx);
                    });
                }
            }),
        )
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<Shutdown> for InterfoldSolWriter<P> {
    type Result = ();

    fn handle(&mut self, _: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<E3StageChanged>
    for InterfoldSolWriter<P>
{
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: E3StageChanged, _: &mut Self::Context) -> Self::Result {
        if !self.effects_enabled {
            return Box::pin(async {});
        }

        Box::pin({
            let e3_id = msg.e3_id.clone();
            let contract_address = self.contract_address;
            let provider = self.provider.clone();
            async move {
                let result = process_e3_failure(provider, contract_address, e3_id.clone()).await;
                match result {
                    Ok(receipt) => {
                        info!(
                            tx=%receipt.transaction_hash,
                            e3_id = %e3_id,
                            "Called processE3Failure"
                        );
                    }
                    Err(err) => {
                        info!(
                            e3_id = %e3_id,
                            "processE3Failure did not succeed (may already be processed): {}",
                            format_evm_error(&err)
                        );
                    }
                }
            }
        })
    }
}
