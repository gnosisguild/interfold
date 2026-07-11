// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::FlushPendingSnapshots;
use actix::{Actor, ActorContext, Addr, Handler, Recipient, ResponseFuture};
use anyhow::{Context, Result};
use e3_utils::MAILBOX_LIMIT;

use crate::{Insert, InsertBatch};

pub struct Batch {
    inserts: Vec<Insert>,
    db: Recipient<InsertBatch>,
}

impl Batch {
    pub fn new(db: impl Into<Recipient<InsertBatch>>, inserts: Vec<Insert>) -> Self {
        Self {
            inserts,
            db: db.into(),
        }
    }
    pub fn spawn(db: impl Into<Recipient<InsertBatch>>, inserts: Vec<Insert>) -> Addr<Self> {
        Self::new(db, inserts).start()
    }
}

impl Actor for Batch {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT);
    }
}

impl Handler<Insert> for Batch {
    type Result = ();
    fn handle(&mut self, msg: Insert, _: &mut Self::Context) -> Self::Result {
        self.inserts.push(msg)
    }
}

impl Handler<FlushPendingSnapshots> for Batch {
    type Result = ResponseFuture<Result<()>>;

    fn handle(&mut self, _: FlushPendingSnapshots, ctx: &mut Self::Context) -> Self::Result {
        let inserts = std::mem::take(&mut self.inserts);
        let db = self.db.clone();
        ctx.stop();
        Box::pin(async move {
            if !inserts.is_empty() {
                db.send(InsertBatch::new(inserts))
                    .await
                    .context("snapshot destination stopped before accepting its final batch")??;
            }
            Ok(())
        })
    }
}
