// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Reads fulfilled VRF responses and starts the existing sortition pipeline.

use crate::contracts::{ICiphernodeRegistry, IRandomnessProvider};
use crate::domain::log_timestamp::from_log_chain_id_to_ts;
use crate::domain::randomness_provider_events::{committee_requested, SortitionRequestContext};
use crate::helpers::EthProvider;
use crate::messages::{EvmEvent, EvmEventProcessor, EvmLog, EvmLogRejected, InterfoldEvmEvent};
use actix::prelude::*;
use alloy::{primitives::Address, providers::Provider, sol_types::SolEvent};
use anyhow::{Context as _, Result};
use e3_utils::MAILBOX_LIMIT;
use std::time::Duration;
use tracing::{debug, error, warn};

const EVENT_FORWARD_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RandomnessProviderSolReader<P> {
    provider: EthProvider<P>,
    registry: Address,
    next: EvmEventProcessor,
}

impl<P: Provider + Clone + 'static> Actor for RandomnessProviderSolReader<P> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

impl<P: Provider + Clone + 'static> RandomnessProviderSolReader<P> {
    pub fn setup(
        next: &EvmEventProcessor,
        provider: EthProvider<P>,
        registry: Address,
    ) -> Addr<Self> {
        Self {
            provider,
            registry,
            next: next.clone(),
        }
        .start()
    }
}

async fn parse_fulfillment<P: Provider + Clone + 'static>(
    provider: EthProvider<P>,
    registry_address: Address,
    log: EvmLog,
) -> Result<Option<EvmEvent>> {
    let block = log
        .log
        .block_number
        .context("randomness log is missing its block number")?;
    let log_index = log
        .log
        .log_index
        .context("randomness log is missing its log index")?;
    if log.log.topics().first() != Some(&IRandomnessProvider::RandomnessFulfilled::SIGNATURE_HASH) {
        return Ok(None);
    }
    let fulfillment = IRandomnessProvider::RandomnessFulfilled::decode_log_data(log.log.data())
        .context("invalid RandomnessFulfilled event")?;

    let registry = ICiphernodeRegistry::new(registry_address, provider.provider());
    let resolved = registry.sortitionSeed(fulfillment.e3Id).call().await?;
    if !resolved.ready {
        warn!(
            e3_id = %fulfillment.e3Id,
            request_id = %fulfillment.requestId,
            "Ignoring randomness that the registry did not accept"
        );
        return Ok(None);
    }
    let request = registry
        .getSortitionRequest(fulfillment.e3Id)
        .call()
        .await?;
    let data = committee_requested(SortitionRequestContext {
        e3_id: fulfillment.e3Id,
        seed: resolved.seed,
        threshold: request.threshold,
        request_block: request.requestBlock,
        committee_deadline: request.committeeDeadline,
        ticket_price: request.ticketPrice,
        chain_id: log.chain_id,
    });
    let timestamp = from_log_chain_id_to_ts(log.timestamp, log_index, log.chain_id);
    Ok(Some(EvmEvent::new(
        log.id,
        data,
        block,
        timestamp,
        log.chain_id,
    )))
}

async fn forward(next: EvmEventProcessor, event: InterfoldEvmEvent) -> Result<()> {
    tokio::time::timeout(EVENT_FORWARD_TIMEOUT, next.send(event))
        .await
        .context("timed out while forwarding a randomness event")?
        .context("randomness event destination stopped")?;
    Ok(())
}

impl<P: Provider + Clone + 'static> Handler<InterfoldEvmEvent> for RandomnessProviderSolReader<P> {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvmEvent, ctx: &mut Self::Context) {
        match msg.clone() {
            InterfoldEvmEvent::Log(log) => {
                debug!(id = %log.id, "processing randomness event");
                let id = log.id;
                let chain_id = log.chain_id;
                let provider = self.provider.clone();
                let registry = self.registry;
                let next = self.next.clone();
                ctx.wait(
                    async move {
                        let parsed = parse_fulfillment(provider, registry, log).await;
                        let event = match parsed {
                            Ok(Some(event)) => InterfoldEvmEvent::Event(event),
                            Ok(None) => InterfoldEvmEvent::Processed(id),
                            Err(parse_error) => {
                                error!(
                                    %id,
                                    chain_id,
                                    error = %parse_error,
                                    "Rejecting a randomness provider log"
                                );
                                InterfoldEvmEvent::Rejected(EvmLogRejected::new(
                                    id,
                                    chain_id,
                                    parse_error.to_string(),
                                ))
                            }
                        };
                        forward(next, event).await
                    }
                    .into_actor(self)
                    .map(|result, _, ctx| {
                        if let Err(forward_error) = result {
                            error!(
                                error = %forward_error,
                                "Randomness event delivery failed; stopping the parser"
                            );
                            ctx.stop();
                        }
                    }),
                );
            }
            marker @ InterfoldEvmEvent::HistoricalSyncComplete(..) => {
                let next = self.next.clone();
                ctx.wait(
                    async move { forward(next, marker).await }
                        .into_actor(self)
                        .map(|result, _, ctx| {
                            if let Err(forward_error) = result {
                                error!(
                                    error = %forward_error,
                                    "Randomness sync marker delivery failed; stopping the parser"
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
