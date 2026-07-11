// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::{Context, Result};
use e3_ciphernode_builder::{CiphernodeBuilder, CiphernodeHandle};
use e3_config::AppConfig;
use e3_crypto::Cipher;
use e3_zk_prover::ZkBackend;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, instrument};

async fn await_startup<F, T>(future: F, timeout: Duration) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    tokio::time::timeout(timeout, future)
        .await
        .with_context(|| format!("ciphernode startup did not complete within {timeout:?}"))?
}

#[instrument(name = "app", skip_all)]
pub async fn execute(config: &AppConfig) -> Result<CiphernodeHandle> {
    let rng = Arc::new(Mutex::new(
        ChaCha20Rng::try_from_os_rng().context("failed to seed ChaCha20 RNG from OS")?,
    ));
    let cipher = Arc::new(Cipher::from_file(&config.key_file()).await?);
    let backend = ZkBackend::new(config.bb_binary(), config.circuits_dir(), config.work_dir());

    let reserve = config.multithread_reserve_threads();
    let concurrent_jobs = config.multithread_concurrent_jobs();
    info!(
        "Ciphernode multithread: reserve_threads={reserve}, concurrent_jobs={}",
        concurrent_jobs
            .map(|n| n.to_string())
            .unwrap_or_else(|| "auto (CPUs - reserve)".to_string())
    );

    let startup_timeout = Duration::from_secs(config.startup_timeout_secs());
    info!(
        startup_timeout_secs = startup_timeout.as_secs(),
        "Ciphernode startup deadline configured"
    );

    let build = CiphernodeBuilder::new(rng.clone(), cipher.clone())
        .with_name(&config.name())
        .with_logging()
        .with_persistence(&config.log_file(), &config.db_file())
        .with_sortition_score()
        .with_chains(config.chains())
        .with_contract_interfold_full()
        .with_contract_bonding_registry()
        .with_multithread_config(reserve, concurrent_jobs)
        .with_max_buffered_evm_events(config.max_buffered_evm_events())
        .with_network_buffer_limits(
            config.max_buffered_net_events(),
            config.max_buffered_net_bytes(),
        )
        .with_contract_ciphernode_registry()
        .with_contract_slashing_manager()
        .with_trbfv()
        .with_zkproof(backend)
        .with_pubkey_aggregation()
        .with_threshold_plaintext_aggregation()
        .with_net(config.peers(), config.quic_port())
        .with_shared_store()
        .with_shared_eventstore()
        .build();
    let node = await_startup(build, startup_timeout).await?;

    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::await_startup;
    use anyhow::{bail, Result};
    use std::time::Duration;

    #[tokio::test]
    async fn startup_deadline_returns_completed_result() -> Result<()> {
        let value =
            await_startup(async { Ok::<_, anyhow::Error>(7) }, Duration::from_secs(1)).await?;
        assert_eq!(value, 7);
        Ok(())
    }

    #[tokio::test]
    async fn startup_deadline_fails_instead_of_waiting_forever() -> Result<()> {
        let error = await_startup(
            std::future::pending::<Result<()>>(),
            Duration::from_millis(5),
        )
        .await
        .expect_err("pending startup must hit its deadline");
        if !error
            .to_string()
            .contains("startup did not complete within 5ms")
        {
            bail!("unexpected timeout error: {error:#}");
        }
        Ok(())
    }
}
