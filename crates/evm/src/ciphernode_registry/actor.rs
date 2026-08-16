// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Ciphernode registry EVM boundary.
//!
//! The actor owns subscription, routing, and in-flight submission state. Contract
//! reads and transactions live in `transactions`; message handling lives in
//! `handlers`.

use crate::contracts::ICiphernodeRegistry;
use crate::domain::ciphernode_registry_events::{
    decode_committee_request, derive_sortition_seed, extractor, extractor_with_sortition_seed,
    legacy_sortition_seed,
};
use crate::domain::error_decoder::{decode_error_from_str, format_evm_error};
use crate::domain::log_timestamp::from_log_chain_id_to_ts;
use crate::helpers::{encode_zk_proof, send_tx_with_retry, EthProvider, ProviderFactory};
use crate::messages::{EvmEvent, EvmEventProcessor, EvmLog, EvmLogRejected, InterfoldEvmEvent};
use crate::{EvmEffectOutbox, EvmEffectOutboxRepositoryFactory, EvmEffectOutboxState};
use actix::prelude::*;
use alloy::{
    primitives::{Address, Bytes, B256, U256},
    providers::{Provider, WalletProvider},
    rpc::types::TransactionReceipt,
};
use anyhow::{Context as _, Result};
use e3_data::{Repositories, Repository};
use e3_events::{
    prelude::*, AggregatorChanged, BusHandle, CommitteeFinalizeRequested,
    DkgFoldAttestationContextEstablished, E3RequestComplete, E3id, EType, EffectsEnabled,
    EventSubscriber, EventType, InterfoldEvent, InterfoldEventData, PublicKeyAggregated, Shutdown,
    TicketGenerated, TicketId, DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
};
use e3_utils::{require_successful_receipt, NotifySync, MAILBOX_LIMIT};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::{debug, error, info, warn};

const EVENT_FORWARD_TIMEOUT: Duration = Duration::from_secs(5);
const ENTROPY_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

#[path = "effects.rs"]
mod effects;
#[path = "handlers.rs"]
mod handlers;

#[allow(unused_imports)]
pub use effects::{fetch_accusation_vote_validity, fetch_dkg_fold_attestation_verifier};

/// Connects to CiphernodeRegistry.sol converting EVM events to InterfoldEvents.
pub struct CiphernodeRegistrySolReader<P> {
    provider: EthProvider<P>,
    provider_factory: Option<ProviderFactory<P>>,
    confirmations: u64,
    next: EvmEventProcessor,
}

impl<P: Provider + Clone + 'static> Actor for CiphernodeRegistrySolReader<P> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

async fn parse_registry_log<P: Provider + Clone + 'static>(
    mut provider: EthProvider<P>,
    provider_factory: Option<ProviderFactory<P>>,
    confirmations: u64,
    log: EvmLog,
) -> (EthProvider<P>, Result<EvmEvent>) {
    let result = async {
        let block = log.log.block_number.context(
            "provider log is missing its block number; pending or malformed logs cannot be ordered",
        )?;
        let log_index = log.log.log_index.context(
            "provider log is missing its log index; malformed logs cannot be ordered deterministically",
        )?;

        let event = if let Some(request) =
            decode_committee_request(log.log.data(), log.log.topics())
        {
            let expected_entropy_block = block
                .checked_add(1)
                .context("committee entropy block overflow")?;
            let seed = if request.entropyBlock == U256::from(expected_entropy_block) {
                let confirmed_at = expected_entropy_block
                    .checked_add(confirmations)
                    .context("committee entropy confirmation height overflow")?;

                tokio::time::timeout(ENTROPY_WAIT_TIMEOUT, async {
                    loop {
                        let read_result = async {
                            let head = provider.provider().get_block_number().await?;
                            if head < confirmed_at {
                                return Ok(None);
                            }
                            provider
                                .provider()
                                .get_block_by_number(expected_entropy_block.into())
                                .await
                        }
                        .await;

                        match read_result {
                            Ok(Some(block)) => {
                                break derive_sortition_seed(block.header.hash, request.e3Id)
                            }
                            Ok(None) => (),
                            Err(error) => {
                                warn!(
                                    e3_id = %request.e3Id,
                                    %error,
                                    "Unable to read the entropy block; reconnecting"
                                );
                                if let Some(factory) = provider_factory.as_ref() {
                                    match factory().await {
                                        Ok(replacement)
                                            if replacement.chain_id() == log.chain_id =>
                                        {
                                            provider = replacement
                                        }
                                        Ok(replacement) => warn!(
                                            e3_id = %request.e3Id,
                                            expected_chain_id = log.chain_id,
                                            actual_chain_id = replacement.chain_id(),
                                            "Refusing an entropy provider for another chain"
                                        ),
                                        Err(reconnect_error) => warn!(
                                            e3_id = %request.e3Id,
                                            error = %reconnect_error,
                                            "Unable to reconnect to the entropy provider"
                                        ),
                                    }
                                }
                            }
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                })
                .await
                .with_context(|| {
                    format!(
                        "committee {} entropy block {} was not readable within {:?}",
                        request.e3Id, expected_entropy_block, ENTROPY_WAIT_TIMEOUT
                    )
                })?
            } else {
                // Older deployments stored the request-time seed in the same event field.
                debug!(
                    e3_id = %request.e3Id,
                    "Replaying a committee request that predates delayed sortition entropy"
                );
                legacy_sortition_seed(request.entropyBlock)
            };

            extractor_with_sortition_seed(log.log.data(), log.log.topics(), log.chain_id, seed)
        } else {
            extractor(log.log.data(), log.log.topics(), log.chain_id)
        }
        .context("contract log matched the CiphernodeRegistry address but could not be decoded")?;

        let timestamp = from_log_chain_id_to_ts(log.timestamp, log_index, log.chain_id);
        Ok(EvmEvent::new(
            log.id,
            event,
            block,
            timestamp,
            log.chain_id,
        ))
    }
    .await;

    (provider, result)
}

async fn forward_registry_event(next: EvmEventProcessor, event: InterfoldEvmEvent) -> Result<()> {
    tokio::time::timeout(EVENT_FORWARD_TIMEOUT, next.send(event))
        .await
        .context("timed out while forwarding a ciphernode registry event")?
        .context("ciphernode registry event destination stopped")?;
    Ok(())
}

impl<P: Provider + Clone + 'static> CiphernodeRegistrySolReader<P> {
    pub fn setup_with_factory(
        next: &EvmEventProcessor,
        provider: EthProvider<P>,
        provider_factory: Option<ProviderFactory<P>>,
        confirmations: u64,
    ) -> Addr<Self> {
        Self {
            provider,
            provider_factory,
            confirmations,
            next: next.clone(),
        }
        .start()
    }
}

impl<P: Provider + Clone + 'static> Handler<InterfoldEvmEvent> for CiphernodeRegistrySolReader<P> {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvmEvent, ctx: &mut Self::Context) -> Self::Result {
        match msg.clone() {
            InterfoldEvmEvent::Log(log) => {
                debug!("processing event({})", msg.get_id());
                let id = log.id;
                let chain_id = log.chain_id;
                let provider = self.provider.clone();
                let provider_factory = self.provider_factory.clone();
                let confirmations = self.confirmations;
                let next = self.next.clone();

                ctx.wait(
                    async move {
                        let (provider, parsed) =
                            parse_registry_log(provider, provider_factory, confirmations, log)
                                .await;
                        let event = match parsed {
                            Ok(event) => InterfoldEvmEvent::Event(event),
                            Err(parse_error) => {
                                error!(
                                    %id,
                                    chain_id,
                                    error = %parse_error,
                                    "Rejecting EVM log and failing the chain ingestion pipeline"
                                );
                                InterfoldEvmEvent::Rejected(EvmLogRejected::new(
                                    id,
                                    chain_id,
                                    parse_error.to_string(),
                                ))
                            }
                        };
                        let result = forward_registry_event(next, event).await;
                        (provider, result)
                    }
                    .into_actor(self)
                    .map(move |(provider, result), actor, ctx| {
                        actor.provider = provider;
                        if let Err(forward_error) = result {
                            error!(
                                %id,
                                chain_id,
                                error = %forward_error,
                                "Ciphernode registry event delivery failed; stopping the parser"
                            );
                            ctx.stop();
                        }
                    }),
                );
            }
            hist @ InterfoldEvmEvent::HistoricalSyncComplete(..) => {
                let next = self.next.clone();
                ctx.wait(
                    async move { forward_registry_event(next, hist).await }
                        .into_actor(self)
                        .map(|result, _, ctx| {
                            if let Err(forward_error) = result {
                                error!(
                                    error = %forward_error,
                                    "Registry sync marker delivery failed; stopping the parser"
                                );
                                ctx.stop();
                            }
                        }),
                );
            }
            _ => (),
        }
    }
}

/// Writer for publishing committees to CiphernodeRegistry.
pub struct CiphernodeRegistrySolWriter<P> {
    provider: EthProvider<P>,
    contract_address: Address,
    bus: BusHandle,
    effects_enabled: bool,
    active_aggregators: HashMap<E3id, bool>,
    request_registries: HashMap<E3id, Address>,
    outbox: EvmEffectOutbox<RegistryEffect>,
    /// Session-local concurrency guard around durable semantic outbox keys.
    submitting: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum RegistryEffect {
    SubmitTicket(TicketGenerated),
    FinalizeCommittee(CommitteeFinalizeRequested),
    PublishCommitteeProof {
        registry: Address,
        event: PublicKeyAggregated,
    },
    PublishCommitteePublicKey {
        registry: Address,
        event: PublicKeyAggregated,
    },
}

impl RegistryEffect {
    fn key(&self) -> String {
        match self {
            Self::SubmitTicket(event) => {
                crate::semantic_effect_key("submit_ticket", &event.e3_id, &event.ticket_id)
            }
            Self::FinalizeCommittee(event) => {
                crate::semantic_effect_key("finalize_committee", &event.e3_id, &())
            }
            Self::PublishCommitteeProof { registry, event } => crate::semantic_effect_key(
                "publish_committee",
                &event.e3_id,
                &(
                    registry,
                    event.pk_commitment,
                    &event.dkg_aggregator_proof,
                    &event.dkg_attestation_bundle,
                ),
            ),
            Self::PublishCommitteePublicKey { registry, event } => crate::semantic_effect_key(
                "publish_committee_public_key",
                &event.e3_id,
                &(registry, &event.pubkey, event.pk_commitment),
            ),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct DrainRegistryOutbox;

#[derive(Message)]
#[rtype(result = "()")]
struct ExecuteRegistryEffect {
    key: String,
    effect: RegistryEffect,
    status: crate::EvmEffectStatus,
}

impl<P: Provider + WalletProvider + Clone + 'static> CiphernodeRegistrySolWriter<P> {
    async fn new(
        bus: &BusHandle,
        provider: EthProvider<P>,
        contract_address: Address,
        request_registries: HashMap<E3id, Address>,
        repository: Repository<EvmEffectOutboxState<RegistryEffect>>,
    ) -> Result<Self> {
        Ok(Self {
            provider,
            contract_address,
            bus: bus.clone(),
            effects_enabled: false,
            active_aggregators: HashMap::new(),
            request_registries,
            outbox: EvmEffectOutbox::load(repository).await?,
            submitting: HashSet::new(),
        })
    }

    async fn attach(
        bus: &BusHandle,
        provider: EthProvider<P>,
        contract_address: Address,
        request_registries: HashMap<E3id, Address>,
        repository: Repository<EvmEffectOutboxState<RegistryEffect>>,
    ) -> Result<Addr<Self>> {
        let addr = CiphernodeRegistrySolWriter::new(
            bus,
            provider,
            contract_address,
            request_registries,
            repository,
        )
        .await?
        .start();

        bus.subscribe_all(
            &[
                EventType::EffectsEnabled,
                EventType::AggregatorChanged,
                EventType::DkgFoldAttestationContextEstablished,
                EventType::PublicKeyAggregated,
                EventType::CommitteeFinalizeRequested,
                EventType::TicketGenerated,
                EventType::E3RequestComplete,
                EventType::Shutdown,
            ],
            addr.clone().into(),
        );
        Ok(addr)
    }

    fn is_active_aggregator_for(&self, e3_id: &E3id) -> bool {
        self.active_aggregators.get(e3_id).copied().unwrap_or(false)
    }
}

/// Wrapper for a reader and writer.
pub struct CiphernodeRegistrySol;

impl CiphernodeRegistrySol {
    pub async fn attach_writer<P>(
        bus: &BusHandle,
        provider: EthProvider<P>,
        contract_address: Address,
        request_registries: HashMap<E3id, Address>,
        repositories: &Repositories,
    ) -> Result<Addr<CiphernodeRegistrySolWriter<P>>>
    where
        P: Provider + WalletProvider + Clone + 'static,
    {
        let signer = provider.provider().default_signer_address();
        let writer_scope = format!("ciphernode_registry/{contract_address}/{signer}");
        let repository = repositories.evm_effect_outbox(&writer_scope, provider.chain_id());
        CiphernodeRegistrySolWriter::attach(
            bus,
            provider,
            contract_address,
            request_registries,
            repository,
        )
        .await
    }
}

impl<P: Provider + WalletProvider + Clone + 'static> Handler<crate::GetEvmWriterHealth>
    for CiphernodeRegistrySolWriter<P>
{
    type Result = ResponseFuture<crate::EvmWriterHealth>;

    fn handle(
        &mut self,
        message: crate::GetEvmWriterHealth,
        _: &mut Self::Context,
    ) -> Self::Result {
        let outbox = self.outbox.clone();
        let chain_id = self.provider.chain_id();
        let contract_address = self.contract_address.to_string();
        let effects_enabled = self.effects_enabled;
        let in_flight_effects = self.submitting.len();
        Box::pin(async move {
            let summary = outbox.summary(message.now_ms).await;
            crate::EvmWriterHealth {
                writer: "ciphernode_registry".to_owned(),
                chain_id,
                contract_address,
                effects_enabled,
                pending_effects: summary.pending_effects,
                oldest_pending_age_ms: summary.oldest_pending_age_ms,
                in_flight_effects,
            }
        })
    }
}
