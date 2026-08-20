// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use actix::prelude::*;
use e3_events::{prelude::*, AggregatorChanged, Die, InterfoldEvent, InterfoldEventData};
use e3_utils::MAILBOX_LIMIT;
use std::collections::HashSet;
use tracing::warn;

use crate::ThresholdPlaintextAggregator;

const MAX_BUFFERED_EVENTS: usize = MAILBOX_LIMIT;
const MAX_BUFFERED_BYTES: u64 = 64 * 1024 * 1024;

pub struct DecryptionshareCreatedBuffer {
    dest: Addr<ThresholdPlaintextAggregator>,
    buffer: Vec<InterfoldEvent>,
    buffered_bytes: u64,
    expelled_parties: HashSet<u64>,
    is_aggregator: bool,
}

impl DecryptionshareCreatedBuffer {
    pub fn new(dest: Addr<ThresholdPlaintextAggregator>) -> Self {
        Self::new_with_aggregator_state(dest, false)
    }

    pub fn new_with_aggregator_state(
        dest: Addr<ThresholdPlaintextAggregator>,
        is_aggregator: bool,
    ) -> Self {
        Self {
            dest,
            buffer: Vec::new(),
            buffered_bytes: 0,
            expelled_parties: HashSet::new(),
            is_aggregator,
        }
    }

    fn forward(dest: &Addr<ThresholdPlaintextAggregator>, event: InterfoldEvent) {
        dest.do_send(event);
    }

    fn flush(&mut self) {
        if !self.is_aggregator {
            return;
        }

        self.buffered_bytes = 0;
        for event in self.buffer.drain(..) {
            match event.get_data() {
                InterfoldEventData::DecryptionshareCreated(data)
                    if !self.expelled_parties.contains(&data.party_id) =>
                {
                    Self::forward(&self.dest, event);
                }
                InterfoldEventData::CommitteeMemberExpelled(data) if data.party_id.is_some() => {
                    Self::forward(&self.dest, event);
                }
                InterfoldEventData::CommitteeMemberExcluded(data) if data.party_id.is_some() => {
                    Self::forward(&self.dest, event);
                }
                InterfoldEventData::E3RequestComplete(_) | InterfoldEventData::Shutdown(_) => {
                    Self::forward(&self.dest, event);
                }
                _ => {}
            }
        }
    }

    fn buffer_event(&mut self, event: InterfoldEvent) {
        let Ok(event_bytes) = bincode::serialized_size(&event) else {
            warn!(event_type = %event.event_type(), "Discarding an event that cannot be measured for standby buffering");
            return;
        };
        let Some(buffered_bytes) = self.buffered_bytes.checked_add(event_bytes) else {
            warn!(event_type = %event.event_type(), "Discarding an event that overflowed the standby buffer size counter");
            return;
        };
        if self.buffer.len() >= MAX_BUFFERED_EVENTS || buffered_bytes > MAX_BUFFERED_BYTES {
            warn!(
                event_type = %event.event_type(),
                buffered_events = self.buffer.len(),
                buffered_bytes = self.buffered_bytes,
                "Discarding an event because the standby decryption-share buffer is full"
            );
            return;
        }

        self.buffer.push(event);
        self.buffered_bytes = buffered_bytes;
    }

    fn retain_buffered(&mut self, mut keep: impl FnMut(&InterfoldEvent) -> bool) {
        self.buffer.retain(|event| keep(event));
        self.buffered_bytes = self
            .buffer
            .iter()
            .filter_map(|event| bincode::serialized_size(event).ok())
            .sum();
    }

    fn clear_buffer(&mut self) {
        self.buffer.clear();
        self.buffered_bytes = 0;
    }
}

impl Actor for DecryptionshareCreatedBuffer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
    }
}

impl Handler<InterfoldEvent> for DecryptionshareCreatedBuffer {
    type Result = ();

    fn handle(&mut self, msg: InterfoldEvent, _ctx: &mut Self::Context) -> Self::Result {
        match msg.get_data() {
            InterfoldEventData::DecryptionshareCreated(data) => {
                if self.expelled_parties.contains(&data.party_id) {
                    return;
                }

                if self.is_aggregator {
                    Self::forward(&self.dest, msg);
                } else {
                    self.buffer_event(msg);
                }
            }
            InterfoldEventData::CommitteeMemberExpelled(data) => {
                let Some(party_id) = data.party_id else {
                    return;
                };

                if !self.expelled_parties.insert(party_id) {
                    return;
                }
                self.retain_buffered(|event| {
                    !matches!(
                        event.get_data(),
                        InterfoldEventData::DecryptionshareCreated(share)
                            if share.party_id == party_id
                    )
                });

                if self.is_aggregator {
                    Self::forward(&self.dest, msg);
                } else {
                    self.buffer_event(msg);
                }
            }
            InterfoldEventData::CommitteeMemberExcluded(data) => {
                let Some(party_id) = data.party_id else {
                    return;
                };

                if !self.expelled_parties.insert(party_id) {
                    return;
                }
                self.retain_buffered(|event| {
                    !matches!(
                        event.get_data(),
                        InterfoldEventData::DecryptionshareCreated(share)
                            if share.party_id == party_id
                    )
                });

                if self.is_aggregator {
                    Self::forward(&self.dest, msg);
                } else {
                    self.buffer_event(msg);
                }
            }
            InterfoldEventData::AggregatorChanged(AggregatorChanged { is_aggregator, .. }) => {
                self.is_aggregator = *is_aggregator;
                Self::forward(&self.dest, msg);
                self.flush();
            }
            InterfoldEventData::E3RequestComplete(_) | InterfoldEventData::Shutdown(_) => {
                self.clear_buffer();
                Self::forward(&self.dest, msg);
            }
            _ => {
                if self.is_aggregator {
                    Self::forward(&self.dest, msg);
                }
            }
        }
    }
}

impl Handler<Die> for DecryptionshareCreatedBuffer {
    type Result = ();

    fn handle(&mut self, _: Die, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}
