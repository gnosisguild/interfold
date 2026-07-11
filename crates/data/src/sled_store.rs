// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::{ShutdownStore, SledDb};
use actix::{Actor, ActorContext, Addr, Handler, ResponseFuture};
use anyhow::{Context, Result};
use e3_events::{BusHandle, EType, ErrorDispatcher, Flush, InterfoldEvent, Unsequenced};
use e3_events::{Get, Insert, InsertBatch, InsertSync, Remove};
use e3_utils::MAILBOX_LIMIT;
use std::path::PathBuf;
use tracing::{error, info};

pub struct SledStore {
    db: Option<SledDb>,
    bus: Box<dyn ErrorDispatcher<InterfoldEvent<Unsequenced>>>,
    write_failure: Option<String>,
}

impl Actor for SledStore {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(MAILBOX_LIMIT)
    }
}

impl SledStore {
    pub fn new<S: 'static>(bus: &BusHandle<S>, path: &PathBuf) -> Result<Addr<Self>> {
        // The generic BusHandle is retained only for structured storage errors;
        // shutdown itself is coordinated explicitly after actor snapshots drain.
        info!("Starting SledStore with {:?}", path);
        let db = SledDb::new(path, "datastore")?;

        let store = Self {
            db: Some(db),
            bus: Box::new(bus.clone()),
            write_failure: None,
        }
        .start();

        Ok(store)
    }

    fn record_write_failure(&mut self, error: &anyhow::Error) {
        if self.write_failure.is_none() {
            self.write_failure = Some(format!("{error:#}"));
        }
    }
}

impl Handler<Insert> for SledStore {
    type Result = ();

    fn handle(&mut self, event: Insert, _: &mut Self::Context) -> Self::Result {
        if let Some(ref mut db) = &mut self.db {
            if let Err(err) = db.insert(event) {
                self.record_write_failure(&err);
                self.bus.err(EType::Data, err)
            }
        }
    }
}

impl Handler<InsertBatch> for SledStore {
    type Result = ();

    fn handle(&mut self, event: InsertBatch, _: &mut Self::Context) -> Self::Result {
        if let Some(ref mut db) = &mut self.db {
            if let Err(err) = db.insert_batch(event.commands()) {
                self.record_write_failure(&err);
                self.bus.err(EType::Data, err)
            }
        }
    }
}

impl Handler<InsertSync> for SledStore {
    type Result = Result<()>;

    fn handle(&mut self, event: InsertSync, _: &mut Self::Context) -> Self::Result {
        if let Some(ref mut db) = &mut self.db {
            if let Err(error) = db.insert(event.into()) {
                self.record_write_failure(&error);
                return Err(error);
            }
        }
        Ok(())
    }
}

impl Handler<Remove> for SledStore {
    type Result = ();

    fn handle(&mut self, event: Remove, _: &mut Self::Context) -> Self::Result {
        if let Some(ref mut db) = &mut self.db {
            if let Err(err) = db.remove(event) {
                self.record_write_failure(&err);
                self.bus.err(EType::Data, err)
            }
        }
    }
}

impl Handler<Get> for SledStore {
    type Result = Option<Vec<u8>>;

    fn handle(&mut self, event: Get, _: &mut Self::Context) -> Self::Result {
        if let Some(ref mut db) = &mut self.db {
            match db.get(event) {
                Ok(v) => v,
                Err(err) => {
                    self.bus.err(EType::Data, err);
                    None
                }
            }
        } else {
            error!("Attempt to get data from dropped db");
            None
        }
    }
}

impl Handler<Flush> for SledStore {
    type Result = ();
    fn handle(&mut self, _: Flush, _: &mut Self::Context) -> Self::Result {
        if let Some(ref db) = self.db {
            if let Err(err) = db.flush() {
                self.record_write_failure(&err);
                self.bus.err(EType::Data, err)
            }
        }
    }
}

impl Handler<ShutdownStore> for SledStore {
    type Result = ResponseFuture<Result<()>>;

    fn handle(&mut self, _: ShutdownStore, ctx: &mut Self::Context) -> Self::Result {
        let db = self.db.take();
        let write_failure = self.write_failure.take();
        ctx.stop();

        Box::pin(async move {
            let db = db.context("SledStore was already closed")?;
            tokio::task::spawn_blocking(move || db.flush())
                .await
                .context("SledStore flush task failed")??;

            if let Some(error) = write_failure {
                anyhow::bail!("SledStore observed a write failure before shutdown: {error}");
            }
            Ok(())
        })
    }
}
