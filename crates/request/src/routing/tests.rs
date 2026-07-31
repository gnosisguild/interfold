// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::*;
use crate::{
    ContextRepositoryFactory, DkgFoldAttestationContextRepositoryFactory, E3ContextSnapshot,
    E3MetaExtension, DKG_FOLD_ATTESTATION_CONTEXT_KEY,
};
use actix::{Actor, Handler};
use async_trait::async_trait;
use e3_data::{InMemStore, RepositoriesFactory};
use e3_events::{
    hlc_factory::HlcFactory, BusHandle, DkgFoldAttestationContext,
    DkgFoldAttestationContextEstablished, EventBus, Sequencer, StoreEventRequested,
    DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct StoreSink;

impl Actor for StoreSink {
    type Context = Context<Self>;
}

impl Handler<StoreEventRequested> for StoreSink {
    type Result = ();

    fn handle(&mut self, _: StoreEventRequested, _: &mut Self::Context) {}
}

struct RecoveryExtension {
    hydrations: Arc<AtomicUsize>,
}

#[async_trait]
impl E3Extension for RecoveryExtension {
    fn on_event(&self, _: &mut E3Context, _: &InterfoldEvent) {}

    async fn hydrate(&self, _: &mut E3Context, _: &E3ContextSnapshot) -> Result<()> {
        self.hydrations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn test_bus() -> BusHandle {
    let event_bus = EventBus::<InterfoldEvent>::default().start();
    let store = StoreSink.start();
    let sequencer = Sequencer::new(&event_bus, store.recipient()).start();
    BusHandle::new(event_bus, sequencer, HlcFactory::new()).enable("router-recovery-test")
}

#[actix::test]
async fn mid_e3_context_and_completed_set_survive_hydration() -> Result<()> {
    let active = E3id::new("7", 31337);
    let complete = E3id::new("6", 31337);
    let store = DataStore::from_in_mem(&InMemStore::new(false).start());
    let repositories = store.repositories();
    let router_store = repositories.router();
    router_store
        .repositories()
        .context(&active)
        .write_sync(&E3ContextSnapshot {
            e3_id: active.clone(),
            recipients: vec!["threshold_keyshare".into()],
            dependencies: vec!["meta".into()],
        })
        .await?;

    let hydrations = Arc::new(AtomicUsize::new(0));
    let params = E3RouterParams {
        extensions: Arc::new(vec![Box::new(RecoveryExtension {
            hydrations: hydrations.clone(),
        })]),
        bus: test_bus(),
        store: router_store,
    };
    let recovered = E3Router::from_snapshot(
        params,
        E3RouterSnapshot {
            contexts: vec![active.clone()],
            completed: HashSet::from([complete.clone()]),
        },
    )
    .await?;

    assert!(recovered.contexts.contains_key(&active));
    assert!(recovered.completed.contains(&complete));
    assert_eq!(hydrations.load(Ordering::SeqCst), 1);

    let roundtrip = recovered.snapshot()?;
    assert_eq!(roundtrip.contexts, vec![active]);
    assert_eq!(roundtrip.completed, HashSet::from([complete]));
    Ok(())
}

#[actix::test]
async fn request_time_attestation_contexts_survive_router_snapshots() -> Result<()> {
    let old_e3 = E3id::new("41", 1);
    let new_e3 = E3id::new("42", 1);
    let legacy_e3 = E3id::new("40", 1);
    let old_context = DkgFoldAttestationContext {
        registry: "0x1111111111111111111111111111111111111111".parse()?,
        verifying_contract: "0x1212121212121212121212121212121212121212".parse()?,
    };
    let new_context = DkgFoldAttestationContext {
        registry: "0x2121212121212121212121212121212121212121".parse()?,
        verifying_contract: "0x2222222222222222222222222222222222222222".parse()?,
    };
    let repositories = DataStore::from_in_mem(&InMemStore::new(false).start()).repositories();
    let router_store = repositories.router();
    let context_repositories = router_store.repositories();

    for (e3_id, context) in [(old_e3.clone(), old_context), (new_e3.clone(), new_context)] {
        context_repositories
            .context(&e3_id)
            .repositories()
            .dkg_fold_attestation_context(&e3_id)
            .write_sync(&DkgFoldAttestationContextEstablished {
                schema_version: DKG_FOLD_ATTESTATION_CONTEXT_SCHEMA_VERSION,
                e3_id,
                context,
            })
            .await?;
    }
    context_repositories
        .context(&old_e3)
        .write_sync(&E3ContextSnapshot {
            e3_id: old_e3.clone(),
            recipients: Vec::new(),
            dependencies: vec!["dkg_fold_attestation_context".into()],
        })
        .await?;
    router_store
        .write_sync(&E3RouterSnapshot {
            contexts: vec![legacy_e3.clone(), old_e3.clone(), new_e3.clone()],
            completed: HashSet::new(),
        })
        .await?;

    let restored = load_dkg_fold_attestation_contexts(&repositories).await?;
    assert_eq!(restored.get(&old_e3), Some(&old_context));
    assert_eq!(restored.get(&new_e3), Some(&new_context));
    assert!(!restored.contains_key(&legacy_e3));

    let extensions: Arc<Vec<Box<dyn E3Extension>>> = Arc::new(vec![E3MetaExtension::create()]);
    let recovered = E3Router::from_snapshot(
        E3RouterParams {
            extensions,
            bus: test_bus(),
            store: router_store,
        },
        E3RouterSnapshot {
            contexts: vec![old_e3.clone()],
            completed: HashSet::new(),
        },
    )
    .await?;
    assert!(recovered.contexts.contains_key(&old_e3));
    assert_eq!(
        recovered
            .contexts
            .get(&old_e3)
            .and_then(|context| context.get_dependency(DKG_FOLD_ATTESTATION_CONTEXT_KEY)),
        Some(&old_context)
    );
    Ok(())
}
