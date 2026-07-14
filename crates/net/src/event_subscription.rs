// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Shared retained-event subscription behavior for network capabilities.

use crate::events::NetEvent;
use tokio::sync::broadcast::{error::RecvError, Receiver};
use tracing::{debug, warn};

/// Receive the next retained network event without treating broadcast lag as channel closure.
///
/// Tokio reports lag as an item-level error and advances the receiver to the oldest event still in
/// the ring buffer. Callers must therefore retry after `Lagged`; returning from their receive task
/// would permanently detach that networking subsystem after a single burst. Only `Closed` is
/// terminal. The warning deliberately contains only a static consumer name and a numeric count so
/// peer- or payload-controlled data cannot create unbounded log fields.
pub(crate) async fn recv_net_event(
    receiver: &mut Receiver<NetEvent>,
    consumer: &'static str,
) -> Option<NetEvent> {
    loop {
        match receiver.recv().await {
            Ok(event) => return Some(event),
            Err(RecvError::Lagged(skipped_events)) => {
                warn!(
                    consumer,
                    skipped_events, "network event receiver lagged; resuming from retained events"
                );
            }
            Err(RecvError::Closed) => {
                debug!(consumer, "network event broadcast channel closed");
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::error::TryRecvError;

    #[tokio::test]
    async fn lagged_receiver_continues_with_retained_and_future_events() {
        let (tx, mut receiver) = tokio::sync::broadcast::channel(1);
        let mut lag_sentinel_control = tx.subscribe();

        tx.send(NetEvent::AllPeersDialed {
            connected: 1,
            total: 2,
        })
        .expect("receiver is active");
        tx.send(NetEvent::AllPeersDialed {
            connected: 2,
            total: 2,
        })
        .expect("receiver is active");

        assert!(matches!(
            lag_sentinel_control.try_recv(),
            Err(TryRecvError::Lagged(skipped)) if skipped > 0
        ));

        let retained = recv_net_event(&mut receiver, "test-consumer")
            .await
            .expect("lag must not terminate the receiver");
        assert!(matches!(
            retained,
            NetEvent::AllPeersDialed {
                connected: 2,
                total: 2
            }
        ));

        tx.send(NetEvent::AllPeersDialed {
            connected: 3,
            total: 3,
        })
        .expect("receiver remains active after lag");
        let future = recv_net_event(&mut receiver, "test-consumer")
            .await
            .expect("receiver must continue processing future events");
        assert!(matches!(
            future,
            NetEvent::AllPeersDialed {
                connected: 3,
                total: 3
            }
        ));
    }

    #[tokio::test]
    async fn closed_receiver_is_terminal() {
        let (tx, mut receiver) = tokio::sync::broadcast::channel::<NetEvent>(1);
        drop(tx);

        assert!(recv_net_event(&mut receiver, "test-consumer")
            .await
            .is_none());
    }
}
