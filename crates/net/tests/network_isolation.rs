// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::time::Duration;

use anyhow::Result;
use e3_config::NetworkProfile;
use e3_net::events::NetEvent;
use e3_net::{Libp2pKeypair, Libp2pNetInterface, NetInterface, NetworkPolicy};
use tokio::time::{sleep, timeout};

fn free_udp_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind UDP socket");
    socket.local_addr().expect("read UDP address").port()
}

#[tokio::test]
async fn mainnet_rejects_a_sepolia_peer_before_network_admission() -> Result<()> {
    assert_rejected(
        NetworkPolicy::new(NetworkProfile::mainnet(), [(1, [1; 20])])?,
        NetworkPolicy::new(NetworkProfile::sepolia(), [(11_155_111, [2; 20])])?,
    )
    .await
}

#[tokio::test]
async fn mainnet_rejects_a_peer_with_a_different_deployment() -> Result<()> {
    assert_rejected(
        NetworkPolicy::new(NetworkProfile::mainnet(), [(1, [1; 20])])?,
        NetworkPolicy::new(NetworkProfile::mainnet(), [(1, [2; 20])])?,
    )
    .await
}

async fn assert_rejected(listener: NetworkPolicy, dialer: NetworkPolicy) -> Result<()> {
    let mainnet_port = free_udp_port();
    let mainnet_key = Libp2pKeypair::generate();
    let mainnet_peer = mainnet_key.peer_id();
    let mut mainnet = Libp2pNetInterface::new(mainnet_key, vec![], Some(mainnet_port), listener)?;
    tokio::spawn(async move { mainnet.start().await });
    sleep(Duration::from_millis(250)).await;

    let address = format!("/ip4/127.0.0.1/udp/{mainnet_port}/quic-v1/p2p/{mainnet_peer}");
    let mut sepolia =
        Libp2pNetInterface::new(Libp2pKeypair::generate(), vec![address], None, dialer)?;
    let handle = sepolia.handle();
    let mut events = handle.rx();
    tokio::spawn(async move { sepolia.start().await });

    timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await? {
                NetEvent::ConnectionEstablished { .. } => {
                    anyhow::bail!("incompatible peer entered the admitted network")
                }
                NetEvent::PeerRejected { reason, .. } => {
                    assert!(reason.contains("network identity"));
                    return anyhow::Ok(());
                }
                _ => {}
            }
        }
    })
    .await
    .expect("timed out waiting for network rejection")?;

    Ok(())
}
