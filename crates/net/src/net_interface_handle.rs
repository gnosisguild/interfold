// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::time::Duration;

use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};

use crate::{
    events::{NetCommand, NetEvent},
    NetworkStatus,
};

/// Sends each network event to the raw channel and, when required, to the application channel.
///
/// The application channel excludes sync and connection-control traffic at the producer.
#[derive(Debug, Clone)]
pub struct NetEventSender {
    raw: broadcast::Sender<NetEvent>,
    application: broadcast::Sender<NetEvent>,
}

impl NetEventSender {
    pub(crate) fn new(raw_capacity: usize, application_capacity: usize) -> Self {
        let (raw, _) = broadcast::channel(raw_capacity);
        let (application, _) = broadcast::channel(application_capacity);
        Self { raw, application }
    }

    /// Sends one event to its required channels.
    pub fn send(&self, event: NetEvent) -> Result<usize, broadcast::error::SendError<NetEvent>> {
        if !event.requires_application_delivery() {
            return self.raw.send(event);
        }

        let raw_result = self.raw.send(event.clone());
        let application_result = self.application.send(event);

        match (raw_result, application_result) {
            (Ok(receivers), _) | (Err(_), Ok(receivers)) => Ok(receivers),
            (Err(error), Err(_)) => Err(error),
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<NetEvent> {
        self.raw.subscribe()
    }

    pub(crate) fn application_subscribe(&self) -> broadcast::Receiver<NetEvent> {
        self.application.subscribe()
    }

    pub(crate) fn len(&self) -> usize {
        self.raw.len()
    }
}

#[derive(Debug)]
pub struct NetInterfaceHandle {
    tx: mpsc::Sender<NetCommand>,
    rx: broadcast::Receiver<NetEvent>,
    application_rx: broadcast::Receiver<NetEvent>,
    status: NetworkStatus,
}
impl NetInterfaceHandle {
    pub(crate) fn new(
        tx: mpsc::Sender<NetCommand>,
        rx: broadcast::Receiver<NetEvent>,
        application_rx: broadcast::Receiver<NetEvent>,
        status: NetworkStatus,
    ) -> Self {
        Self {
            tx,
            rx,
            application_rx,
            status,
        }
    }

    pub fn status(&self) -> NetworkStatus {
        self.status.clone()
    }
}

pub trait NetInterface: Sized {
    fn tx(&self) -> mpsc::Sender<NetCommand>;
    fn rx(&self) -> broadcast::Receiver<NetEvent>;
    /// Returns a receiver that contains only application-delivery events.
    fn application_rx(&self) -> broadcast::Receiver<NetEvent>;
    fn status(&self) -> NetworkStatus;
    fn handle(&self) -> NetInterfaceHandle {
        NetInterfaceHandle::from(self)
    }
}

#[derive(Debug, Clone)]
/// Allow Net events and commands to be bridged between nodes. This is used for testing purposes to
/// simulate libp2p without running libp2p.
pub struct NetChannelBridge {
    cmd_tx: broadcast::Sender<NetCommand>,
    tx: mpsc::Sender<NetCommand>,
    event_tx: NetEventSender,
}

impl NetInterfaceHandle {
    pub fn from(interface: &impl NetInterface) -> Self {
        Self {
            tx: interface.tx(),
            rx: interface.rx(),
            application_rx: interface.application_rx(),
            status: interface.status(),
        }
    }
}
impl NetInterface for NetInterfaceHandle {
    fn rx(&self) -> broadcast::Receiver<NetEvent> {
        self.rx.resubscribe()
    }

    fn tx(&self) -> mpsc::Sender<NetCommand> {
        self.tx.clone()
    }

    fn application_rx(&self) -> broadcast::Receiver<NetEvent> {
        self.application_rx.resubscribe()
    }

    fn status(&self) -> NetworkStatus {
        self.status.clone()
    }
}

/// This creates a channel bridge which allows for network events to be connected between test nodes
pub fn create_channel_bridge() -> (NetInterfaceHandle, NetChannelBridge) {
    create_channel_bridge_with_application_event_capacity(crate::DEFAULT_MAX_BUFFERED_NET_EVENTS)
}

/// Creates a test channel bridge whose application channel uses the specified capacity.
pub fn create_channel_bridge_with_application_event_capacity(
    application_event_capacity: usize,
) -> (NetInterfaceHandle, NetChannelBridge) {
    assert!(
        application_event_capacity > 0,
        "application event channel capacity must be greater than zero"
    );
    let (m_cmd_tx, mut m_cmd_rx) = mpsc::channel::<NetCommand>(1000);
    let event_tx = NetEventSender::new(1000, application_event_capacity);
    let (b_cmd_tx, _) = broadcast::channel(1000);

    let tx = b_cmd_tx.clone();
    let startup_event_tx = event_tx.clone();
    let keep_alive = b_cmd_tx.subscribe();

    // Bridge from mpsc channel to broadcast channel simulating AllPeersDialed for each node
    tokio::spawn(async move {
        let _rx_guard = keep_alive;
        sleep(Duration::from_millis(100)).await;
        let _ = startup_event_tx.send(NetEvent::AllPeersDialed {
            connected: 0,
            total: 0,
        });
        while let Some(cmd) = m_cmd_rx.recv().await {
            let _ = tx.send(cmd);
        }
    });

    let handle = NetInterfaceHandle {
        tx: m_cmd_tx.clone(),
        rx: event_tx.subscribe(),
        application_rx: event_tx.application_subscribe(),
        status: NetworkStatus::new(0),
    };

    let inverted = NetChannelBridge {
        tx: m_cmd_tx,
        cmd_tx: b_cmd_tx,
        event_tx,
    };

    (handle, inverted)
}

pub trait NetInterfaceInverted: Sized {
    fn tx(&self) -> mpsc::Sender<NetCommand>;
    fn event_tx(&self) -> NetEventSender; //U
    fn event_rx(&self) -> broadcast::Receiver<NetEvent>;
    fn cmd_tx(&self) -> broadcast::Sender<NetCommand>;
    fn cmd_rx(&self) -> broadcast::Receiver<NetCommand>; //U

    fn into_handle_inverted(self) -> NetChannelBridge {
        NetChannelBridge {
            tx: self.tx(),
            event_tx: self.event_tx(),
            cmd_tx: self.cmd_tx(),
        }
    }
}

impl NetInterfaceInverted for NetChannelBridge {
    fn tx(&self) -> mpsc::Sender<NetCommand> {
        self.tx.clone()
    }

    fn cmd_rx(&self) -> broadcast::Receiver<NetCommand> {
        self.cmd_tx.subscribe()
    }
    fn event_tx(&self) -> NetEventSender {
        self.event_tx.clone()
    }
    fn cmd_tx(&self) -> broadcast::Sender<NetCommand> {
        self.cmd_tx.clone()
    }
    fn event_rx(&self) -> broadcast::Receiver<NetEvent> {
        self.event_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use crate::events::GossipData;

    use super::*;

    #[test]
    fn application_event_send_succeeds_with_either_channel_subscribed() {
        let raw_only = NetEventSender::new(1, 1);
        let mut raw_rx = raw_only.subscribe();
        raw_only
            .send(NetEvent::GossipData(GossipData::GossipBytes(vec![1])))
            .expect("raw delivery must succeed without an application subscriber");
        assert!(matches!(raw_rx.try_recv(), Ok(NetEvent::GossipData(_))));

        let application_only = NetEventSender::new(1, 1);
        let mut application_rx = application_only.application_subscribe();
        application_only
            .send(NetEvent::GossipData(GossipData::GossipBytes(vec![2])))
            .expect("application delivery must succeed without a raw subscriber");
        assert!(matches!(
            application_rx.try_recv(),
            Ok(NetEvent::GossipData(_))
        ));
    }
}
