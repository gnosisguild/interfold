// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use alloy::{
    network::Ethereum,
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder},
    rpc::types::{BlockNumberOrTag, Filter, Log},
    sol_types::SolEvent,
};
use eyre::Result;
use futures::stream::StreamExt;
use futures_util::future::FutureExt;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::RwLock;

type EventHandler =
    Box<dyn Fn(&Log) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct EventListener {
    provider: Arc<dyn Provider<Ethereum>>,
    filter: Filter,
    handlers: Arc<RwLock<HashMap<B256, Vec<EventHandler>>>>,
    /// Handlers that receive every log, whatever its topic.
    raw_handlers: Arc<RwLock<Vec<EventHandler>>>,
}

impl EventListener {
    pub fn new(provider: Arc<dyn Provider<Ethereum>>, filter: Filter) -> Self {
        Self {
            provider,
            filter,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            raw_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a handler that receives EVERY log this listener sees, undecoded.
    ///
    /// The typed `add_event_handler` above binds a handler to one event signature, which is right
    /// for reacting to a known event. This one exists for the opposite job: retaining logs whose
    /// meaning belongs to the caller, not to this crate — an index that serves a client's own
    /// queries has to keep events it has no ABI for.
    pub async fn add_raw_handler<F, Fut>(&self, handler: F)
    where
        F: Fn(Log) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let wrapped = Box::new(move |log: &Log| {
            let handler = Arc::clone(&handler);
            let log = log.clone();
            async move { handler(log).await }.boxed()
        });

        self.raw_handlers.write().await.push(wrapped);
    }

    pub async fn add_event_handler<E, F, Fut>(&self, handler: F)
    where
        E: SolEvent + Send + Clone + 'static,
        F: Fn(E) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let signature = E::SIGNATURE_HASH;
        let handler = Arc::new(handler);
        let wrapped_handler = Box::new(move |log: &Log| {
            let handler = Arc::clone(&handler);
            let log = log.clone();
            async move {
                let decoded = log.log_decode::<E>()?;
                let event = decoded.inner.data;
                handler(event.clone()).await
            }
            .boxed()
        });

        self.handlers
            .write()
            .await
            .entry(signature)
            .or_insert_with(Vec::new)
            .push(wrapped_handler);
    }

    pub async fn listen(&self) -> Result<()> {
        let mut stream = self
            .provider
            .subscribe_logs(&self.filter)
            .await?
            .into_stream();
        while let Some(log) = stream.next().await {
            // Raw handlers run first and are awaited, not spawned: they are what persists the log,
            // and a typed handler that reacts to it should not be able to observe a state the
            // index has not recorded yet.
            //
            // The guard is released before each await, exactly as the typed path below does. Held
            // across the handler it would deadlock anything that registers a handler from inside
            // one, and would serialise every handler's IO into this subscription loop.
            let raw_count = self.raw_handlers.read().await.len();
            for i in 0..raw_count {
                let fut = {
                    let guard = self.raw_handlers.read().await;
                    let Some(handler) = guard.get(i) else { break };
                    handler(&log)
                };
                if let Err(e) = fut.await {
                    eprintln!("Error in raw log handler: {e:?}");
                }
            }

            if let Some(topic0) = log.topic0() {
                let topic_val = *topic0;
                if let Some(handlers) = self.handlers.read().await.get(topic0) {
                    for handler in handlers {
                        let log_clone = log.clone();
                        let fut = handler(&log_clone);
                        tokio::spawn(async move {
                            // Spawn the future so that the handlers are processed concurrently
                            if let Err(e) = fut.await {
                                eprintln!("Error processing event 0x{:x}: {:?}", topic_val, e);
                            }
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// The current chain head, used to bound a catch-up range.
    pub async fn head_block(&self) -> Result<u64> {
        Ok(self.provider.get_block_number().await?)
    }

    /// Replay historical logs through the registered handlers, inclusive of both bounds.
    ///
    /// `listen` subscribes from the head, so every log emitted while this process was down is
    /// invisible to it — a subscription has no memory. Anything that treats the store as a
    /// complete picture of the chain (an API serving a frontend, rather than a coordinator
    /// reacting to live events) therefore needs this to close the gap on boot and after a
    /// reconnect.
    ///
    /// Two differences from the live path, both deliberate:
    ///
    /// - Handlers run SEQUENTIALLY and in block order. The live path spawns them concurrently,
    ///   which is fine when events arrive one at a time, but replaying a range that way would
    ///   interleave writes for the same round and let a later event lose to an earlier one.
    /// - A handler error aborts the catch-up rather than being logged and skipped. Continuing
    ///   would advance the caller's cursor past an event that was never applied, turning a
    ///   transient failure into a permanent hole.
    ///
    /// Queried in windows of `chunk` blocks because hosted providers cap `eth_getLogs` ranges.
    pub async fn catch_up(&self, from_block: u64, to_block: u64, chunk: u64) -> Result<u64> {
        if from_block > to_block {
            return Ok(0);
        }

        let chunk = chunk.max(1);
        let mut processed = 0u64;
        let mut start = from_block;

        while start <= to_block {
            let end = start.saturating_add(chunk - 1).min(to_block);

            let filter = self
                .filter
                .clone()
                .from_block(BlockNumberOrTag::Number(start))
                .to_block(BlockNumberOrTag::Number(end));

            let mut logs = self.provider.get_logs(&filter).await?;

            // Providers are not required to return logs in order, and the handlers below are
            // order-sensitive: `block_number`/`log_index` is the chain's own ordering.
            logs.sort_by_key(|log| (log.block_number, log.log_index));

            for log in &logs {
                let raw_count = self.raw_handlers.read().await.len();
                for i in 0..raw_count {
                    let fut = {
                        let guard = self.raw_handlers.read().await;
                        let Some(handler) = guard.get(i) else { break };
                        handler(log)
                    };
                    fut.await?;
                }

                let Some(topic0) = log.topic0() else {
                    processed += 1;
                    continue;
                };
                let topic_val = *topic0;

                // The read guard is taken per log and released before awaiting the handler, so a
                // handler that registers further handlers cannot deadlock against it.
                let count = self
                    .handlers
                    .read()
                    .await
                    .get(topic0)
                    .map(|h| h.len())
                    .unwrap_or(0);

                for i in 0..count {
                    let fut = {
                        let guard = self.handlers.read().await;
                        let Some(handlers) = guard.get(&topic_val) else {
                            break;
                        };
                        handlers[i](log)
                    };
                    fut.await?;
                }

                processed += 1;
            }

            start = end.saturating_add(1);
        }

        Ok(processed)
    }

    pub fn provider(&self) -> Arc<dyn Provider<Ethereum>> {
        self.provider.clone()
    }

    /// Create a contract listener that will listen to events from all addresses.
    pub async fn create_contract_listener(rpc_url: &str, addresses: &[&str]) -> Result<Self> {
        let provider = Arc::new(ProviderBuilder::new().connect(rpc_url).await?);

        let address = addresses
            .iter()
            .map(|a| a.parse::<Address>().map_err(|e| eyre::eyre!("{e}")))
            .collect::<Result<Vec<_>>>()?;
        let filter = Filter::new()
            .address(address)
            .from_block(BlockNumberOrTag::Latest);
        Ok(EventListener::new(provider, filter))
    }
}
