// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Ciphernode release compatibility and on-chain acknowledgement.

use crate::{
    contracts::{IBondingRegistry, IInterfold, INodeReleaseRegistry},
    helpers::{transaction_nonce_guard, EthProvider},
};
use alloy::{
    primitives::Address,
    providers::{Provider, WalletProvider},
};
use anyhow::{ensure, Context, Result};
use e3_config::current_node_release;
use e3_utils::require_successful_receipt;
use tracing::info;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReleasePolicy {
    required_protocol_version: u32,
    required_node_generation: u32,
}

fn validate_release_policy(policy: ReleasePolicy) -> Result<()> {
    let release = current_node_release();
    ensure!(
        release.protocol_version == policy.required_protocol_version,
        "ciphernode protocol version {} does not match required version {}",
        release.protocol_version,
        policy.required_protocol_version
    );
    ensure!(
        release.node_generation >= policy.required_node_generation,
        "ciphernode generation {} is below required generation {}",
        release.node_generation,
        policy.required_node_generation
    );
    Ok(())
}

/// Verify this binary and acknowledge it before contract readers start.
pub async fn ensure_node_release<P>(
    provider: EthProvider<P>,
    interfold_address: Address,
    bonding_address: Address,
    ciphernode_registry_address: Address,
) -> Result<()>
where
    P: Provider + WalletProvider + Clone + 'static,
{
    let operator = provider.provider().default_signer_address();
    let interfold = IInterfold::new(interfold_address, provider.provider());
    let (controller_address, configured_bonding, configured_ciphernode_registry) =
        tokio::try_join!(
            async { interfold.nodeReleaseRegistry().call().await },
            async { interfold.bondingRegistry().call().await },
            async { interfold.ciphernodeRegistry().call().await },
        )
        .context("failed to read Interfold release dependencies")?;
    ensure!(
        controller_address != Address::ZERO,
        "Interfold has no node release controller; keep this ciphernode stopped until the protocol upgrade is complete"
    );
    ensure!(
        configured_bonding == bonding_address
            && configured_ciphernode_registry == ciphernode_registry_address,
        "configured contracts do not match Interfold dependencies: BondingRegistry {} vs {}; CiphernodeRegistry {} vs {}",
        bonding_address,
        configured_bonding,
        ciphernode_registry_address,
        configured_ciphernode_registry
    );

    let controller = INodeReleaseRegistry::new(controller_address, provider.provider());
    let release = current_node_release();
    let release_id = release.release_id();
    let (
        required_protocol_version,
        required_node_generation,
        acknowledged_release,
        controller_bonding,
        controller_ciphernode_registry,
    ) = tokio::try_join!(
        async { controller.requiredProtocolVersion().call().await },
        async { controller.requiredNodeGeneration().call().await },
        async { controller.operatorNodeRelease(operator).call().await },
        async { controller.bondingRegistry().call().await },
        async { controller.ciphernodeRegistry().call().await },
    )?;
    ensure!(
        controller_bonding == bonding_address
            && controller_ciphernode_registry == ciphernode_registry_address,
        "node release controller is bound to different protocol contracts"
    );
    validate_release_policy(ReleasePolicy {
        required_protocol_version,
        required_node_generation,
    })
    .with_context(|| {
        format!(
            "ciphernode {} ({release_id}) cannot join the active protocol",
            release.version()
        )
    })?;

    let bonding = IBondingRegistry::new(bonding_address, provider.provider());
    let (registered, active) = tokio::try_join!(
        async { bonding.isRegistered(operator).call().await },
        async { bonding.isActive(operator).call().await },
    )?;
    if acknowledged_release.releaseId == release_id
        && acknowledged_release.protocolVersion == release.protocol_version
        && acknowledged_release.nodeGeneration == release.node_generation
        && (!registered || active)
    {
        info!(
            version = release.version(),
            protocol_version = release.protocol_version,
            node_generation = release.node_generation,
            "Ciphernode release is accepted"
        );
        return Ok(());
    }

    info!(
        version = release.version(),
        protocol_version = release.protocol_version,
        node_generation = release.node_generation,
        "Acknowledging ciphernode release"
    );
    let _nonce_guard = transaction_nonce_guard(&provider).await;
    let current_nonce = provider
        .provider()
        .get_transaction_count(operator)
        .pending()
        .await?;
    let pending = controller
        .acknowledgeNodeRelease(
            release_id,
            release.protocol_version,
            release.node_generation,
        )
        .nonce(current_nonce)
        .send()
        .await?;
    drop(_nonce_guard);
    let receipt = pending.get_receipt().await?;
    require_successful_receipt("acknowledge ciphernode release", &receipt)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_release_at_required_versions() {
        let release = current_node_release();
        validate_release_policy(ReleasePolicy {
            required_protocol_version: release.protocol_version,
            required_node_generation: release.node_generation,
        })
        .unwrap();
    }

    #[test]
    fn rejects_newer_requirements() {
        let release = current_node_release();
        let base = ReleasePolicy {
            required_protocol_version: release.protocol_version,
            required_node_generation: release.node_generation,
        };
        assert!(validate_release_policy(ReleasePolicy {
            required_node_generation: release.node_generation + 1,
            ..base
        })
        .is_err());
        assert!(validate_release_policy(ReleasePolicy {
            required_protocol_version: release.protocol_version + 1,
            ..base
        })
        .is_err());
    }
}
