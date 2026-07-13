// SPDX-License-Identifier: LGPL-3.0-only

//! Bound incoming historical-sync work and expire abandoned storage requests.

use super::*;

impl NetSyncManager {
    pub(in crate::actors::net_sync_manager) fn publish_net_ready(&self) -> Result<()> {
        info!("NetSyncManager: publishing NetReady");
        self.bus.publish_without_context(NetReady::new())?;
        Ok(())
    }

    pub(in crate::actors::net_sync_manager) fn request_capacity_error(
        &self,
        peer: &PeerId,
    ) -> Option<&'static str> {
        if self.requests.len() >= MAX_IN_FLIGHT_SYNC_REQUESTS {
            return Some("too many in-flight sync requests");
        }
        let peer_requests = self
            .requests
            .values()
            .filter(|request| &request.peer == peer)
            .count();
        if peer_requests >= MAX_IN_FLIGHT_SYNC_REQUESTS_PER_PEER {
            return Some("too many in-flight sync requests from this peer");
        }
        None
    }

    pub(in crate::actors::net_sync_manager) fn expire_sync_request(&mut self, id: CorrelationId) {
        let Some(pending) = self.requests.remove(&id) else {
            return;
        };
        warn!(
            peer = %pending.peer,
            correlation_id = %id,
            timeout_ms = INCOMING_SYNC_REQUEST_TIMEOUT.as_millis(),
            "Incoming historical-sync storage query timed out"
        );
        if let Err(error) = pending.responder.respond(ProtocolResponse::Error(
            "historical sync request timed out".to_string(),
        )) {
            warn!(
                peer = %pending.peer,
                correlation_id = %id,
                %error,
                "Failed to send historical-sync timeout response"
            );
        }
    }
}
