// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::{ensure, Result};
use e3_data::{Repositories, Repository};
use e3_events::{
    AggregateId, DkgFoldAttestationContextEstablished, E3Stage, E3id, RequestRouterCheckpoint,
    StoreKeys,
};
use std::collections::HashMap;

use crate::{E3ContextSnapshot, E3Meta, E3RouterSnapshot};

pub trait MetaRepositoryFactory {
    fn meta(&self, e3_id: &E3id) -> Repository<E3Meta>;
}

impl MetaRepositoryFactory for Repositories {
    fn meta(&self, e3_id: &E3id) -> Repository<E3Meta> {
        Repository::new(self.store.scope(StoreKeys::meta(e3_id)))
    }
}

pub trait DkgFoldAttestationContextRepositoryFactory {
    fn dkg_fold_attestation_context(
        &self,
        e3_id: &E3id,
    ) -> Repository<DkgFoldAttestationContextEstablished>;
}

impl DkgFoldAttestationContextRepositoryFactory for Repositories {
    fn dkg_fold_attestation_context(
        &self,
        e3_id: &E3id,
    ) -> Repository<DkgFoldAttestationContextEstablished> {
        Repository::new(
            self.store
                .scope(StoreKeys::dkg_fold_attestation_context(e3_id)),
        )
    }
}

pub trait ContextRepositoryFactory {
    fn context(&self, e3_id: &E3id) -> Repository<E3ContextSnapshot>;
}

impl ContextRepositoryFactory for Repositories {
    fn context(&self, e3_id: &E3id) -> Repository<E3ContextSnapshot> {
        Repository::new(self.store.scope(StoreKeys::context(e3_id)))
    }
}

pub trait RouterRepositoryFactory {
    fn router(&self) -> Repository<E3RouterSnapshot>;
    fn request_router_checkpoint(&self) -> Repository<RequestRouterCheckpoint>;
}

impl RouterRepositoryFactory for Repositories {
    fn router(&self) -> Repository<E3RouterSnapshot> {
        Repository::new(self.store.scope(StoreKeys::router()))
    }

    fn request_router_checkpoint(&self) -> Repository<RequestRouterCheckpoint> {
        Repository::new(self.store.scope(StoreKeys::request_router_checkpoint()))
    }
}

pub trait E3LifecycleRepositoryFactory {
    fn e3_lifecycle(&self) -> Repository<HashMap<E3id, E3Stage>>;
}

impl E3LifecycleRepositoryFactory for Repositories {
    fn e3_lifecycle(&self) -> Repository<HashMap<E3id, E3Stage>> {
        Repository::new(self.store.scope(StoreKeys::e3_lifecycle()))
    }
}

/// Create the atomic router checkpoint for a store written by an older binary.
///
/// The migration is safe only when no E3 is active. An active request can have router and context
/// snapshots from different events, so the node must stop instead of accepting an unsafe cursor.
pub async fn ensure_request_router_checkpoint(
    repositories: &Repositories,
    aggregate_ids: impl IntoIterator<Item = AggregateId>,
) -> Result<()> {
    let checkpoint_store = repositories.request_router_checkpoint();
    if checkpoint_store.read().await?.is_some() {
        return Ok(());
    }

    let legacy_snapshot = repositories.router().read().await?;
    let (contexts, completed) = legacy_snapshot
        .map(|snapshot| (snapshot.contexts, snapshot.completed))
        .unwrap_or_default();
    let lifecycle = repositories
        .e3_lifecycle()
        .read()
        .await?
        .unwrap_or_default();
    let active_lifecycle = lifecycle
        .iter()
        .filter(|(_, stage)| !matches!(stage, E3Stage::Complete | E3Stage::Failed))
        .map(|(e3_id, _)| e3_id.to_string())
        .collect::<Vec<_>>();

    ensure!(
        contexts.is_empty() && active_lifecycle.is_empty(),
        "cannot initialize the request-router recovery checkpoint while E3 requests are active; active router contexts: {:?}; active lifecycle entries: {:?}",
        contexts,
        active_lifecycle
    );

    let mut replay_cursors = HashMap::new();
    for aggregate_id in aggregate_ids {
        let cursor = Repository::<u64>::new(
            repositories
                .store
                .scope(StoreKeys::aggregate_seq(aggregate_id)),
        )
        .read()
        .await?
        .unwrap_or(0);
        replay_cursors.insert(aggregate_id, cursor);
    }

    checkpoint_store
        .write_sync(&RequestRouterCheckpoint {
            contexts,
            completed,
            replay_cursors,
        })
        .await
}
