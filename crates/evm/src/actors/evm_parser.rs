// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::{Actor, Handler};
use alloy::primitives::{LogData, B256};
use anyhow::{Context as _, Result};
use e3_events::InterfoldEventData;
use e3_utils::MAILBOX_LIMIT;
use tracing::{debug, error};

use crate::domain::log_timestamp::from_log_chain_id_to_ts;
use crate::messages::{EvmEvent, EvmEventProcessor, EvmLog, EvmLogRejected, InterfoldEvmEvent};

pub type ExtractorFn<E> = fn(&LogData, &[B256], u64) -> Option<E>;

pub struct EvmParser {
    next: EvmEventProcessor,
    extractor: ExtractorFn<InterfoldEventData>,
}

impl Actor for EvmParser {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

impl EvmParser {
    pub fn new(next: &EvmEventProcessor, extractor: ExtractorFn<InterfoldEventData>) -> Self {
        Self {
            next: next.clone(),
            extractor,
        }
    }
}

fn parse_log(log: EvmLog, extractor: ExtractorFn<InterfoldEventData>) -> Result<EvmEvent> {
    let block = log.log.block_number.context(
        "provider log is missing its block number; pending or malformed logs cannot be ordered",
    )?;
    let log_index = log.log.log_index.context(
        "provider log is missing its log index; malformed logs cannot be ordered deterministically",
    )?;
    let event = extractor(log.log.data(), log.log.topics(), log.chain_id).context(
        "contract log matched a configured address but could not be decoded; refusing to advance",
    )?;
    let timestamp = from_log_chain_id_to_ts(log.timestamp, log_index, log.chain_id);
    Ok(EvmEvent::new(log.id, event, block, timestamp, log.chain_id))
}

impl Handler<InterfoldEvmEvent> for EvmParser {
    type Result = ();
    fn handle(&mut self, msg: InterfoldEvmEvent, _ctx: &mut Self::Context) -> Self::Result {
        match msg.clone() {
            InterfoldEvmEvent::Log(log) => {
                debug!("processing event({})", msg.get_id());
                let id = log.id;
                let chain_id = log.chain_id;
                match parse_log(log, self.extractor) {
                    Ok(event) => self.next.do_send(InterfoldEvmEvent::Event(event)),
                    Err(parse_error) => {
                        error!(
                            %id,
                            chain_id,
                            error = %parse_error,
                            "Rejecting EVM log and failing the chain ingestion pipeline"
                        );
                        self.next
                            .do_send(InterfoldEvmEvent::Rejected(EvmLogRejected::new(
                                id,
                                chain_id,
                                parse_error.to_string(),
                            )));
                    }
                }
            }
            hist @ InterfoldEvmEvent::HistoricalSyncComplete(..) => self.next.do_send(hist),
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::{Actor, Context, Handler};
    use alloy::rpc::types::Log;
    use e3_events::TestEvent;
    use tokio::sync::mpsc;

    fn test_extractor(_: &LogData, _: &[B256], _: u64) -> Option<InterfoldEventData> {
        Some(TestEvent::new("parsed", 1).into())
    }

    fn rejected_extractor(_: &LogData, _: &[B256], _: u64) -> Option<InterfoldEventData> {
        None
    }

    fn log(block_number: Option<u64>, log_index: Option<u64>) -> EvmLog {
        EvmLog::new(
            Log {
                block_number,
                log_index,
                ..Default::default()
            },
            1,
            10,
        )
    }

    #[test]
    fn parser_requires_block_number() {
        let error = parse_log(log(None, Some(0)), test_extractor).unwrap_err();
        assert!(error.to_string().contains("missing its block number"));
    }

    #[test]
    fn parser_requires_log_index() {
        let error = parse_log(log(Some(1), None), test_extractor).unwrap_err();
        assert!(error.to_string().contains("missing its log index"));
    }

    #[test]
    fn parser_rejects_failed_contract_decode() {
        let error = parse_log(log(Some(1), Some(0)), rejected_extractor).unwrap_err();
        assert!(error.to_string().contains("could not be decoded"));
    }

    #[test]
    fn parser_converts_valid_log() {
        let source = log(Some(7), Some(3));
        let id = source.id;
        let event = parse_log(source, test_extractor).unwrap();
        assert_eq!(event.get_id(), id);
        let (_, _, block) = event.split();
        assert_eq!(block, 7);
    }

    struct Collector(mpsc::UnboundedSender<InterfoldEvmEvent>);

    impl Actor for Collector {
        type Context = Context<Self>;
    }

    impl Handler<InterfoldEvmEvent> for Collector {
        type Result = ();

        fn handle(&mut self, msg: InterfoldEvmEvent, _: &mut Self::Context) {
            let _ = self.0.send(msg);
        }
    }

    #[actix::test]
    async fn parser_propagates_an_explicit_rejection() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let collector = Collector(tx).start().recipient();
        let parser = EvmParser::new(&collector, rejected_extractor).start();
        let malformed = log(Some(7), Some(3));
        let expected_id = malformed.id;

        parser
            .send(InterfoldEvmEvent::Log(malformed))
            .await
            .unwrap();

        let rejected = rx.recv().await.expect("parser rejection");
        assert!(matches!(
            rejected,
            InterfoldEvmEvent::Rejected(EvmLogRejected { id, chain_id: 1, .. }) if id == expected_id
        ));
    }
}
