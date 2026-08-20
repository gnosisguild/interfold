// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use libp2p::{swarm::ConnectionId, Multiaddr, PeerId};

const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(30);
const REJECTION_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug)]
pub(crate) struct PendingPeer {
    pub(crate) connection_id: ConnectionId,
    pub(crate) remote_address: Multiaddr,
    pub(crate) direction: &'static str,
    pub(crate) connections: u32,
    established_at: Instant,
}

#[derive(Default)]
pub(crate) struct PeerAdmission {
    pending: HashMap<PeerId, PendingPeer>,
    admitted: HashSet<PeerId>,
    rejected_until: HashMap<PeerId, Instant>,
}

impl PeerAdmission {
    pub(crate) fn stage(&mut self, peer: PeerId, pending: PendingPeer) -> bool {
        self.prune_rejections();
        if self.is_rejected(&peer) {
            return false;
        }
        self.pending.insert(peer, pending);
        true
    }

    pub(crate) fn pending(
        connection_id: ConnectionId,
        remote_address: Multiaddr,
        direction: &'static str,
        connections: u32,
    ) -> PendingPeer {
        PendingPeer {
            connection_id,
            remote_address,
            direction,
            connections,
            established_at: Instant::now(),
        }
    }

    pub(crate) fn admit(&mut self, peer: PeerId) -> Option<PendingPeer> {
        let pending = self.pending.remove(&peer)?;
        self.rejected_until.remove(&peer);
        self.admitted.insert(peer);
        Some(pending)
    }

    /// Reject a peer and return true for the first rejection in the TTL window.
    pub(crate) fn reject(&mut self, peer: PeerId) -> bool {
        self.pending.remove(&peer);
        self.admitted.remove(&peer);
        let first = !self.is_rejected(&peer);
        self.rejected_until
            .insert(peer, Instant::now() + REJECTION_TTL);
        first
    }

    pub(crate) fn is_admitted(&self, peer: &PeerId) -> bool {
        self.admitted.contains(peer)
    }

    pub(crate) fn closed(&mut self, peer: &PeerId, remaining_connections: u32) {
        if remaining_connections == 0 {
            self.pending.remove(peer);
            self.admitted.remove(peer);
        }
    }

    pub(crate) fn expired_pending(&mut self) -> Vec<(PeerId, PendingPeer)> {
        let now = Instant::now();
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.established_at) >= IDENTIFY_TIMEOUT)
            .map(|(peer, pending)| (*peer, pending.clone()))
            .collect();
        for (peer, _) in &expired {
            self.reject(*peer);
        }
        expired
    }

    fn is_rejected(&self, peer: &PeerId) -> bool {
        self.rejected_until
            .get(peer)
            .is_some_and(|until| *until > Instant::now())
    }

    fn prune_rejections(&mut self) {
        let now = Instant::now();
        self.rejected_until.retain(|_, until| *until > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_peer_is_not_staged_again_during_ttl() {
        let peer = PeerId::random();
        let mut admission = PeerAdmission::default();
        assert!(admission.reject(peer));
        assert!(!admission.reject(peer));
        assert!(!admission.stage(
            peer,
            PeerAdmission::pending(
                ConnectionId::new_unchecked(1),
                "/ip4/127.0.0.1/udp/1/quic-v1".parse().unwrap(),
                "inbound",
                1,
            )
        ));
    }
}
