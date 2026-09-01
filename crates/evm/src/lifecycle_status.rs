// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Finalized on-chain lifecycle reads for startup recovery.

use crate::{contracts::IInterfold, EthProvider};
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    primitives::{Address, U256},
    providers::Provider,
};
use anyhow::{bail, Context, Result};
use e3_events::{E3Stage, E3id, FailureReason};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalE3Lifecycle {
    pub stage: E3Stage,
    pub failure_reason: Option<FailureReason>,
}

/// Result of comparing a persisted E3 with finalized Ethereum state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinalizedE3Lifecycle {
    /// The E3 exists at chain head, but its request block is not finalized yet.
    PendingFinality,
    /// The finalized block contains the E3 and is safe to use for irreversible recovery changes.
    Canonical(CanonicalE3Lifecycle),
}

fn classify_finalized_stage(finalized: E3Stage, head: E3Stage) -> Result<Option<E3Stage>> {
    if finalized != E3Stage::None {
        return Ok(Some(finalized));
    }
    if head != E3Stage::None {
        return Ok(None);
    }
    bail!(
        "persisted request context references an E3 that is absent at finalized state and chain head"
    )
}

fn decode_stage(value: u8) -> Result<E3Stage> {
    Ok(match value {
        0 => E3Stage::None,
        1 => E3Stage::Requested,
        2 => E3Stage::CommitteeFinalized,
        3 => E3Stage::KeyPublished,
        4 => E3Stage::CiphertextReady,
        5 => E3Stage::Complete,
        6 => E3Stage::Failed,
        _ => bail!("unknown on-chain E3 stage {value}"),
    })
}

fn decode_failure_reason(value: u8) -> Result<FailureReason> {
    Ok(match value {
        1 => FailureReason::CommitteeFormationTimeout,
        2 => FailureReason::InsufficientCommitteeMembers,
        3 => FailureReason::DKGTimeout,
        4 => FailureReason::DKGInvalidShares,
        5 => FailureReason::NoInputsReceived,
        6 => FailureReason::ComputeTimeout,
        7 => FailureReason::ComputeProviderExpired,
        8 => FailureReason::ComputeProviderFailed,
        9 => FailureReason::RequesterCancelled,
        10 => FailureReason::DecryptionTimeout,
        11 => FailureReason::DecryptionInvalidShares,
        12 => FailureReason::VerificationFailed,
        _ => bail!("unknown on-chain E3 failure reason {value}"),
    })
}

/// Read the lifecycle state from Ethereum's finalized block.
///
/// Recovery uses finalized state because removing a persisted request context is irreversible for
/// that local node. A near-head reorganization must not make the node discard live E3 state.
pub async fn fetch_finalized_e3_lifecycle<P>(
    provider: &EthProvider<P>,
    interfold_address: Address,
    e3_id: &E3id,
) -> Result<FinalizedE3Lifecycle>
where
    P: Provider + Clone,
{
    let raw_e3_id: U256 = e3_id
        .clone()
        .try_into()
        .with_context(|| format!("invalid E3 ID {e3_id}"))?;
    let block = BlockId::Number(BlockNumberOrTag::Finalized);
    let contract = IInterfold::new(interfold_address, provider.provider());
    let finalized_stage = decode_stage(
        contract
            .getE3Stage(raw_e3_id)
            .block(block)
            .call()
            .await
            .with_context(|| format!("failed to read finalized stage for E3 {e3_id}"))?,
    )?;
    let head_stage = if finalized_stage == E3Stage::None {
        decode_stage(
            contract
                .getE3Stage(raw_e3_id)
                .call()
                .await
                .with_context(|| format!("failed to read head stage for E3 {e3_id}"))?,
        )?
    } else {
        finalized_stage.clone()
    };
    let Some(stage) = classify_finalized_stage(finalized_stage, head_stage).with_context(|| {
        format!("persisted request context references unknown on-chain E3 {e3_id}")
    })?
    else {
        return Ok(FinalizedE3Lifecycle::PendingFinality);
    };

    let failure_reason = if stage == E3Stage::Failed {
        Some(decode_failure_reason(
            contract
                .getFailureReason(raw_e3_id)
                .block(block)
                .call()
                .await
                .with_context(|| {
                    format!("failed to read finalized failure reason for E3 {e3_id}")
                })?,
        )?)
    } else {
        None
    };

    Ok(FinalizedE3Lifecycle::Canonical(CanonicalE3Lifecycle {
        stage,
        failure_reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_codes_reject_unknown_values() {
        assert_eq!(decode_stage(6).unwrap(), E3Stage::Failed);
        assert_eq!(
            decode_failure_reason(5).unwrap(),
            FailureReason::NoInputsReceived
        );
        assert!(decode_stage(7).is_err());
        assert!(decode_failure_reason(0).is_err());
    }

    #[test]
    fn head_only_e3_waits_for_finality() {
        assert_eq!(
            classify_finalized_stage(E3Stage::None, E3Stage::Requested).unwrap(),
            None
        );
        assert_eq!(
            classify_finalized_stage(E3Stage::Requested, E3Stage::Requested).unwrap(),
            Some(E3Stage::Requested)
        );
        assert!(classify_finalized_stage(E3Stage::None, E3Stage::None).is_err());
    }
}
