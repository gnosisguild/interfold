// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::*;

#[actix::test]
async fn historical_evm_collection_fails_when_any_chain_disconnects() {
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    sender.send(historical_batch(1, 2)).await.unwrap();
    drop(sender);

    let error = collect_historical_evm_events(receiver, &evm_config(&[1, 2, 3]))
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "historical EVM event channel closed before chains reported: [2, 3]"
    );
}

#[actix::test]
async fn historical_evm_collection_returns_only_after_every_chain_reports() {
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    sender.send(historical_batch(2, 3)).await.unwrap();
    sender.send(historical_batch(1, 2)).await.unwrap();

    let events = collect_historical_evm_events(receiver, &evm_config(&[1, 2]))
        .await
        .unwrap();

    assert_eq!(events.len(), 5);
}
