// SPDX-License-Identifier: LGPL-3.0-only

use super::*;

/// Builder for [`E3Router`].
pub struct E3RouterBuilder {
    pub bus: BusHandle,
    pub extensions: Vec<Box<dyn E3Extension>>,
    pub store: Repository<E3RouterSnapshot>,
}

impl E3RouterBuilder {
    pub fn with(mut self, listener: Box<dyn E3Extension>) -> Self {
        self.extensions.push(listener);
        self
    }

    pub async fn build(self) -> Result<Addr<E3Router>> {
        let legacy_snapshot: Option<E3RouterSnapshot> = self.store.read().await?;
        let recovery_store = self.store.repositories().request_router_checkpoint();
        let recovery_checkpoint = recovery_store.read().await?;
        let (snapshot, replay_cursors) = match recovery_checkpoint {
            Some(checkpoint) => (
                Some(E3RouterSnapshot {
                    contexts: checkpoint.contexts,
                    completed: checkpoint.completed,
                }),
                checkpoint.replay_cursors,
            ),
            None => (legacy_snapshot, HashMap::new()),
        };
        let params = E3RouterParams {
            extensions: self.extensions.into(),
            bus: self.bus.clone(),
            store: self.store.clone(),
            replay_cursors,
            recovery_store,
        };

        let router = match snapshot {
            Some(snapshot) => E3Router::from_snapshot(params, snapshot).await?,
            None => E3Router::from_params(params),
        };

        let addr = router.start();
        self.bus.subscribe(EventType::All, addr.clone().recipient());
        Ok(addr)
    }
}
