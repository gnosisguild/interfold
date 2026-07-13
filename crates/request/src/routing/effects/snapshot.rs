// SPDX-License-Identifier: LGPL-3.0-only

use super::*;

#[derive(Serialize, Deserialize)]
pub struct E3RouterSnapshot {
    pub(in crate::actors::router) contexts: Vec<E3id>,
    pub(in crate::actors::router) completed: HashSet<E3id>,
}

impl Snapshot for E3Router {
    type Snapshot = E3RouterSnapshot;

    fn snapshot(&self) -> Result<Self::Snapshot> {
        Ok(Self::Snapshot {
            contexts: self.contexts.keys().cloned().collect(),
            completed: self.completed.clone(),
        })
    }
}

impl Checkpoint for E3Router {
    fn repository(&self) -> &Repository<E3RouterSnapshot> {
        &self.store
    }
}

#[async_trait]
impl FromSnapshotWithParams for E3Router {
    type Params = E3RouterParams;

    async fn from_snapshot(params: Self::Params, snapshot: Self::Snapshot) -> Result<Self> {
        let mut contexts = HashMap::new();
        let repositories = params.store.repositories();

        for e3_id in snapshot.contexts {
            let Some(context_snapshot) = repositories.context(&e3_id).read().await? else {
                continue;
            };

            contexts.insert(
                e3_id.clone(),
                E3Context::from_snapshot(
                    E3ContextParams {
                        repository: repositories.context(&e3_id),
                        e3_id: e3_id.clone(),
                        extensions: params.extensions.clone(),
                    },
                    context_snapshot,
                )
                .await?,
            );
        }

        Ok(E3Router {
            contexts,
            completed: snapshot.completed,
            extensions: params.extensions,
            buffer: EventBuffer::default(),
            bus: params.bus,
            store: params.store,
        })
    }
}
