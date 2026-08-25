// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use e3_sdk::indexer::SharedStore;

use super::{
    database::SledDB,
    log_repo::LogRepository,
    repo::{CrispE3Repository, CurrentRoundRepository},
};

pub struct AppData {
    db: SharedStore<SledDB>,
}

impl AppData {
    pub fn new(db: SharedStore<SledDB>) -> Self {
        Self { db }
    }

    pub fn e3(&self, e3_id: impl ToString) -> CrispE3Repository<SledDB> {
        CrispE3Repository::new(self.db.clone(), e3_id)
    }

    pub fn current_round(&self) -> CurrentRoundRepository<SledDB> {
        CurrentRoundRepository::new(self.db.clone())
    }

    pub fn logs(&self) -> LogRepository<SledDB> {
        LogRepository::new(self.db.clone())
    }
}
