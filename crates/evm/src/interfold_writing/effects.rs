// SPDX-License-Identifier: LGPL-3.0-only

//! Interfold contract reads and transaction effects.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::actors::interfold_sol_writer) enum MarkFailureOutcome {
    Marked,
    NotDue,
    StageAdvanced,
}

pub(in crate::actors::interfold_sol_writer) async fn read_aggregation_failure_stage<
    P: Provider + WalletProvider + Clone,
>(
    provider: EthProvider<P>,
    contract_address: Address,
    e3_id: E3id,
) -> Result<Option<E3Stage>> {
    let e3_id: U256 = e3_id.try_into()?;
    let contract = IInterfold::new(contract_address, provider.provider());
    let stage = contract.getE3Stage(e3_id).call().await?;
    Ok(match stage {
        2 => Some(E3Stage::CommitteeFinalized),
        4 => Some(E3Stage::CiphertextReady),
        _ => None,
    })
}

pub(in crate::actors::interfold_sol_writer) async fn read_failure_deadline<
    P: Provider + WalletProvider + Clone,
>(
    provider: EthProvider<P>,
    contract_address: Address,
    e3_id: E3id,
    stage: E3Stage,
) -> Result<u64> {
    let e3_id: U256 = e3_id.try_into()?;
    let contract = IInterfold::new(contract_address, provider.provider());
    let deadlines = contract.getDeadlines(e3_id).call().await?;
    let deadline = match stage {
        E3Stage::CommitteeFinalized => deadlines.dkgDeadline,
        E3Stage::CiphertextReady => deadlines.decryptionDeadline,
        _ => anyhow::bail!("stage {stage:?} does not have an aggregation failure deadline"),
    };
    deadline
        .try_into()
        .map_err(|_| anyhow::anyhow!("E3 deadline does not fit in u64"))
}

pub(in crate::actors::interfold_sol_writer) async fn mark_e3_failed_if_due<
    P: Provider + WalletProvider + Clone,
>(
    provider: EthProvider<P>,
    contract_address: Address,
    e3_id: E3id,
    expected_stage: E3Stage,
) -> Result<MarkFailureOutcome> {
    let raw_e3_id: U256 = e3_id.clone().try_into()?;
    let contract = IInterfold::new(contract_address, provider.provider());
    let current_stage = contract.getE3Stage(raw_e3_id).call().await?;
    if current_stage != failure_stage_code(&expected_stage)? {
        return Ok(MarkFailureOutcome::StageAdvanced);
    }

    let condition = contract.checkFailureCondition(raw_e3_id).call().await?;
    if !condition.canFail {
        return Ok(MarkFailureOutcome::NotDue);
    }

    info!(e3_id = %e3_id, stage = ?expected_stage, "markE3Failed() after canonical deadline");
    let _nonce_guard = transaction_nonce_guard(&provider).await;
    let from_address = provider.provider().default_signer_address();
    let current_nonce = provider
        .provider()
        .get_transaction_count(from_address)
        .pending()
        .await?;
    let contract = IInterfold::new(contract_address, provider.provider());
    let builder = contract.markE3Failed(raw_e3_id).nonce(current_nonce);
    let pending = builder.send().await?;
    drop(_nonce_guard);
    let receipt = pending.get_receipt().await?;
    require_successful_receipt("mark E3 failed", &receipt)?;
    Ok(MarkFailureOutcome::Marked)
}

fn failure_stage_code(stage: &E3Stage) -> Result<u8> {
    match stage {
        E3Stage::CommitteeFinalized => Ok(2),
        E3Stage::CiphertextReady => Ok(4),
        _ => anyhow::bail!("stage {stage:?} is not watched for aggregation failure"),
    }
}

pub(in crate::actors::interfold_sol_writer) async fn publish_plaintext_output<
    P: Provider + WalletProvider + Clone,
>(
    provider: EthProvider<P>,
    contract_address: Address,
    e3_id: E3id,
    decrypted_output: Vec<u8>,
    decryption_aggregator_proof: Option<&Proof>,
) -> Result<TransactionReceipt> {
    let e3_id: U256 = e3_id.try_into()?;

    // Skip mode creates a non-empty mock-only C7 placeholder before this boundary.
    let proof = encode_zk_proof(decryption_aggregator_proof.ok_or_else(|| {
        anyhow::anyhow!("mandatory decryption aggregator proof payload missing")
    })?)?;

    send_tx_with_retry(
        "publishPlaintextOutput",
        &["CiphertextOutputNotPublished"],
        || {
            info!("publishPlaintextOutput() e3_id={:?}", e3_id);
            let decrypted_output = Bytes::from(decrypted_output.clone());
            let proof = proof.clone();
            let provider = provider.clone();

            async move {
                let _nonce_guard = transaction_nonce_guard(&provider).await;
                let from_address = provider.provider().default_signer_address();
                let current_nonce = provider
                    .provider()
                    .get_transaction_count(from_address)
                    .pending()
                    .await?;
                let contract = IInterfold::new(contract_address, provider.provider());
                let builder = contract
                    .publishPlaintextOutput(e3_id, decrypted_output, proof)
                    .nonce(current_nonce);
                let pending = builder.send().await?;
                drop(_nonce_guard);
                let receipt = pending.get_receipt().await?;
                require_successful_receipt("publish plaintext output", &receipt)?;
                Ok(receipt)
            }
        },
    )
    .await
}

pub(in crate::actors::interfold_sol_writer) async fn should_publish_plaintext<
    P: Provider + WalletProvider + Clone,
>(
    provider: EthProvider<P>,
    contract_address: Address,
    e3_id: E3id,
) -> Result<bool> {
    let e3_id: U256 = e3_id.try_into()?;
    let contract = IInterfold::new(contract_address, provider.provider());
    let e3 = contract.getE3(e3_id).call().await?;
    Ok(e3.plaintextOutput.is_empty())
}

pub(in crate::actors::interfold_sol_writer) async fn process_e3_failure<
    P: Provider + WalletProvider + Clone,
>(
    provider: EthProvider<P>,
    contract_address: Address,
    e3_id: E3id,
) -> Result<TransactionReceipt> {
    let e3_id: U256 = e3_id.try_into()?;

    info!("processE3Failure() e3_id={:?}", e3_id);

    let _nonce_guard = transaction_nonce_guard(&provider).await;
    let from_address = provider.provider().default_signer_address();
    let current_nonce = provider
        .provider()
        .get_transaction_count(from_address)
        .pending()
        .await?;
    let contract = IInterfold::new(contract_address, provider.provider());
    let builder = contract.processE3Failure(e3_id).nonce(current_nonce);
    let pending = builder.send().await?;
    drop(_nonce_guard);
    let receipt = pending.get_receipt().await?;
    require_successful_receipt("process E3 failure", &receipt)?;
    Ok(receipt)
}
