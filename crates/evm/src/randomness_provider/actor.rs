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
use alloy::{
    eips::BlockId,
    primitives::{Address, U256},
    providers::Provider,
    sol_types::SolEvent,
};
use anyhow::{bail, Context as _, Result};
use e3_utils::MAILBOX_LIMIT;
use std::{future::Future, time::Duration};
use tracing::{debug, error, warn};

const EVENT_FORWARD_TIMEOUT: Duration = Duration::from_secs(5);
const REGISTRY_ACCEPTANCE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct RandomnessProviderSolReader<P> {
    provider: EthProvider<P>,
    registry: Address,
    next: EvmEventProcessor,
}

#[derive(Debug)]
struct AcceptedSortitionContext {
    seed: U256,
    threshold: [u32; 2],
    request_block: U256,
    committee_deadline: U256,
    ticket_price: U256,
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
    let block_hash = log
        .log
        .block_hash
        .context("randomness log is missing its block hash")?;
    let log_index = log
        .log
        .log_index
        .context("randomness log is missing its log index")?;
    if log.log.topics().first() != Some(&IRandomnessProvider::RandomnessFulfilled::SIGNATURE_HASH) {
        return Ok(None);
    }
    let fulfillment = IRandomnessProvider::RandomnessFulfilled::decode_log_data(log.log.data())
        .context("invalid RandomnessFulfilled event")?;

    let Some(accepted) = await_registry_acceptance(
        REGISTRY_ACCEPTANCE_TIMEOUT,
        fulfillment.e3Id,
        fulfillment.requestId,
        block,
        read_accepted_sortition(
            &provider,
            registry_address,
            fulfillment.e3Id,
            fulfillment.requestId,
            BlockId::hash_canonical(block_hash),
            block,
        ),
    )
    .await?
    else {
        return Ok(None);
    };

    let data = committee_requested(SortitionRequestContext {
        e3_id: fulfillment.e3Id,
        seed: accepted.seed,
        threshold: accepted.threshold,
        request_block: accepted.request_block,
        committee_deadline: accepted.committee_deadline,
        ticket_price: accepted.ticket_price,
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

async fn await_registry_acceptance<T, F>(
    timeout: Duration,
    e3_id: U256,
    request_id: U256,
    block: u64,
    future: F,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    tokio::time::timeout(timeout, future)
        .await
        .with_context(|| {
            format!(
                "timed out after {timeout:?} while verifying randomness request {request_id} for E3 {e3_id} at block {block}"
            )
        })?
}

async fn read_accepted_sortition<P: Provider + Clone + 'static>(
    provider: &EthProvider<P>,
    registry_address: Address,
    e3_id: U256,
    request_id: U256,
    event_block: BlockId,
    block: u64,
) -> Result<Option<AcceptedSortitionContext>> {
    let registry = ICiphernodeRegistry::new(registry_address, provider.provider());
    let pinned = match registry
        .sortitionSeed(e3_id)
        .block(event_block.clone())
        .call()
        .await
    {
        Ok(resolved) if !resolved.ready => return Ok(None),
        Ok(resolved) => registry
            .getSortitionRequest(e3_id)
            .block(event_block)
            .call()
            .await
            .map(|request| (resolved, request))
            .map_err(anyhow::Error::from),
        Err(error) => Err(error.into()),
    };

    let (resolved, request) = match pinned {
        Ok(context) => context,
        Err(pinned_error) => {
            warn!(
                e3_id = %e3_id,
                request_id = %request_id,
                event_block = block,
                error = %pinned_error,
                "Could not read the fulfillment block; checking retained registry state"
            );
            let resolved = registry
                .sortitionSeed(e3_id)
                .call()
                .await
                .with_context(|| {
                    format!(
                        "failed to read the accepted sortition seed after block {block}: {pinned_error}"
                    )
                })?;
            if !resolved.ready {
                bail!(
                    "registry acceptance for randomness request {} at block {} is not verifiable",
                    request_id,
                    block
                );
            }
            let request = registry
                .getSortitionRequest(e3_id)
                .call()
                .await
                .context("failed to read the retained sortition request")?;
            (resolved, request)
        }
    };

    Ok(Some(AcceptedSortitionContext {
        seed: resolved.seed,
        threshold: request.threshold,
        request_block: request.requestBlock,
        committee_deadline: request.committeeDeadline,
        ticket_price: request.ticketPrice,
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::{
        primitives::Bytes, providers::ProviderBuilder, sol_types::SolValue,
        transports::mock::Asserter,
    };

    async fn provider(asserter: &Asserter) -> EthProvider<impl Provider + Clone> {
        asserter.push_success(&"0x1");
        EthProvider::new(ProviderBuilder::new().connect_mocked_client(asserter.clone()))
            .await
            .expect("mock chain ID must decode")
    }

    #[tokio::test]
    async fn reads_accepted_state_at_the_event_block() {
        let asserter = Asserter::new();
        let provider = provider(&asserter).await;
        asserter.push_success(&Bytes::from((true, U256::from(9)).abi_encode()));
        asserter.push_success(&Bytes::from(
            ([2u32, 3u32], U256::from(4), U256::from(5), U256::from(6)).abi_encode(),
        ));

        let accepted = read_accepted_sortition(
            &provider,
            Address::ZERO,
            U256::from(7),
            U256::from(8),
            BlockId::number(10),
            10,
        )
        .await
        .expect("event-block state must decode")
        .expect("ready state must be accepted");

        assert_eq!(accepted.seed, U256::from(9));
        assert_eq!(accepted.threshold, [2, 3]);
        assert_eq!(accepted.committee_deadline, U256::from(5));
    }

    #[tokio::test]
    async fn rejects_unverifiable_fulfillment_state() {
        let asserter = Asserter::new();
        let provider = provider(&asserter).await;
        asserter.push_failure_msg("historical state unavailable");
        asserter.push_success(&Bytes::from((false, U256::ZERO).abi_encode()));

        let error = read_accepted_sortition(
            &provider,
            Address::ZERO,
            U256::from(7),
            U256::from(8),
            BlockId::number(10),
            10,
        )
        .await
        .expect_err("uncertain state must fail closed");

        assert!(error.to_string().contains("not verifiable"));
    }

    #[tokio::test]
    async fn ignores_canonically_unusable_fulfillment() {
        let asserter = Asserter::new();
        let provider = provider(&asserter).await;
        asserter.push_success(&Bytes::from((false, U256::ZERO).abi_encode()));

        let accepted = read_accepted_sortition(
            &provider,
            Address::ZERO,
            U256::from(7),
            U256::from(8),
            BlockId::number(10),
            10,
        )
        .await
        .expect("canonical rejection must be processed");

        assert!(accepted.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn bounds_registry_acceptance_reads() {
        let error = await_registry_acceptance(
            Duration::from_secs(15),
            U256::from(7),
            U256::from(8),
            10,
            std::future::pending::<Result<()>>(),
        )
        .await
        .expect_err("a stalled RPC read must time out");

        let message = error.to_string();
        assert!(message.contains("timed out after 15s"), "{message}");
        assert!(message.contains("request 8"), "{message}");
        assert!(message.contains("E3 7"), "{message}");
    }
}
