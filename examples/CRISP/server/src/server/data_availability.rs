// SPDX-License-Identifier: LGPL-3.0-only

//! Persistent publication jobs for CRISP's large encrypted objects.

use crate::{config::Config, server::models::e3_id_to_u256};
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    primitives::{keccak256, Address, Bytes, B256, U256},
    providers::{Provider, ProviderBuilder},
    signers::{local::PrivateKeySigner, SignerSync},
    sol,
    sol_types::SolValue,
};
use e3_data_availability::{
    AvailPublisher, AvailReader, DataAvailabilityPublisher, DataAvailabilityReader, DataReference,
    PendingPublication, ProofStatus,
};
use e3_evm_helpers::contracts::{E3Stage, InterfoldContractFactory, InterfoldRead, InterfoldWrite};
use evm_helpers::{CRISPContract, SimulateError};
use serde::{Deserialize, Serialize};
use sled::{Db, Tree};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::warn;

const JOB_POLL_INTERVAL: Duration = Duration::from_secs(30);
const JOB_STEP_TIMEOUT: Duration = Duration::from_secs(120);
const JOB_STATUS_REFRESH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct InputRejected(&'static str);

fn reject_input(message: &'static str) -> anyhow::Error {
    anyhow::Error::new(InputRejected(message))
}

fn duration_u64(value: U256, name: &str) -> anyhow::Result<u64> {
    value
        .try_into()
        .map_err(|_| anyhow::anyhow!("{name} does not fit in u64"))
}

fn minimum_input_duration(
    randomness_window: u64,
    sortition_window: u64,
    dkg_window: u64,
    voting_window: u64,
    finalization_window: u64,
) -> anyhow::Result<u64> {
    randomness_window
        .checked_add(sortition_window)
        .and_then(|value| value.checked_add(dkg_window))
        .and_then(|value| value.checked_add(voting_window))
        .and_then(|value| value.checked_add(finalization_window))
        .ok_or_else(|| anyhow::anyhow!("required CRISP input duration overflows u64"))
}

/// Return a stable client message only when the caller's ballot was conclusively rejected.
pub fn input_rejection_message(error: &anyhow::Error) -> Option<&'static str> {
    for cause in error.chain() {
        if let Some(rejection) = cause.downcast_ref::<InputRejected>() {
            return Some(rejection.0);
        }
        if matches!(cause.downcast_ref::<SimulateError>(), Some(SimulateError::Reverted(_))) {
            return Some("The vote proof or ciphertext was rejected");
        }
    }
    None
}

sol! {
    struct InputEnvelope {
        bytes noirProof;
        address slotAddress;
        bytes32 encryptedVoteCommitment;
        bytes32 encryptedVoteHash;
        uint40 parentIndexPlusOne;
        bytes availabilityProof;
    }

    struct InputCommitmentEnvelope {
        bytes noirProof;
        address slotAddress;
        bytes32 encryptedVoteCommitment;
        bytes32 encryptedVoteHash;
        uint40 parentIndexPlusOne;
        bytes availabilityAttestation;
    }

    enum StoredE3Stage {
        None,
        Requested,
        CommitteeFinalized,
        KeyPublished,
        CiphertextReady,
        Complete,
        Failed
    }

    #[sol(rpc)]
    interface ICrispAvailabilityState {
        function isInputCommitted(
            uint256 e3Id,
            bytes32 encryptedVoteHash,
            bytes32 commitment,
            address slotAddress,
            uint40 parentIndexPlusOne
        ) external view returns (bool);
        function isInputPublished(
            uint256 e3Id,
            bytes32 encryptedVoteHash,
            bytes32 commitment,
            address slotAddress,
            uint40 parentIndexPlusOne
        ) external view returns (bool);
    }

    #[sol(rpc)]
    interface IInterfoldAvailabilityState {
        function getE3Stage(uint256 e3Id) external view returns (StoredE3Stage);
    }
}

fn decode_input_envelope(encoded: &[u8]) -> anyhow::Result<InputEnvelope> {
    Ok(InputEnvelope::abi_decode_params_validate(encoded)?)
}

fn encode_input_commitment_envelope(envelope: &InputCommitmentEnvelope) -> Vec<u8> {
    envelope.abi_encode_params()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JobKind {
    Input {
        e3_id: String,
        staged_envelope: Vec<u8>,
        #[serde(default = "no_deadline")]
        deadline: u64,
        #[serde(default = "no_deadline")]
        commitment_deadline: u64,
    },
    Output {
        e3_id: String,
        ciphertext_commitment: [u8; 32],
        compute_proof: Vec<u8>,
        #[serde(default = "no_deadline")]
        deadline: u64,
    },
}

const fn no_deadline() -> u64 {
    u64::MAX
}

impl JobKind {
    fn deadline(&self) -> u64 {
        match self {
            Self::Input { deadline, .. } | Self::Output { deadline, .. } => *deadline,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum JobState {
    Created,
    AwaitingCommitment {
        ethereum_payload: Vec<u8>,
    },
    Committed {
        transaction_hash: String,
    },
    AwaitingProof {
        publication: PendingPublication,
        #[serde(default)]
        commitment_transaction_hash: Option<String>,
    },
    Ready {
        ethereum_payload: Vec<u8>,
        #[serde(default)]
        commitment_transaction_hash: Option<String>,
    },
    Submitted {
        transaction_hash: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AvailabilityJob {
    id: String,
    content_hash: [u8; 32],
    object: Vec<u8>,
    kind: JobKind,
    state: JobState,
}

#[derive(Clone, Debug, Serialize)]
pub struct AvailabilityJobView {
    pub job_id: String,
    pub status: String,
    pub tx_hash: Option<String>,
    pub encoded_proof: Option<String>,
    pub message: Option<String>,
}

/// Durable work item created when Ethereum accepts an input reference.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailableInputReference {
    pub e3_id: String,
    pub content_hash: [u8; 32],
    pub availability_block: u32,
    pub availability_leaf_index: u128,
    pub index: u64,
    pub commitment: [u8; 32],
    pub slot: [u8; 20],
    pub parent_index_plus_one: u64,
}

impl AvailableInputReference {
    fn key(&self) -> String {
        format!("{}:{}", self.e3_id, self.index)
    }

    pub fn data_reference(&self) -> DataReference {
        DataReference {
            content_hash: self.content_hash,
            block_number: self.availability_block,
            leaf_index: self.availability_leaf_index,
        }
    }
}

impl From<&AvailabilityJob> for AvailabilityJobView {
    fn from(job: &AvailabilityJob) -> Self {
        let (status, tx_hash, encoded_proof, message) = match &job.state {
            JobState::AwaitingCommitment { ethereum_payload } => (
                "ready_for_commitment",
                None,
                Some(format!("0x{}", hex::encode(ethereum_payload))),
                None,
            ),
            JobState::Created => ("pending_commitment", None, None, None),
            JobState::Committed { transaction_hash } => (
                "pending_availability",
                Some(transaction_hash.clone()),
                None,
                None,
            ),
            JobState::AwaitingProof {
                commitment_transaction_hash,
                ..
            }
            | JobState::Ready {
                commitment_transaction_hash,
                ..
            } => (
                "pending_availability",
                commitment_transaction_hash.clone(),
                None,
                None,
            ),
            JobState::Submitted { transaction_hash } => (
                "success",
                (transaction_hash != "already-finalized").then(|| transaction_hash.clone()),
                None,
                None,
            ),
            JobState::Failed { message } => ("failed_broadcast", None, None, Some(message.clone())),
        };
        Self {
            job_id: job.id.clone(),
            status: status.to_owned(),
            tx_hash,
            encoded_proof,
            message,
        }
    }
}

enum Backend {
    Mock,
    Avail {
        publisher: Arc<AvailPublisher>,
        reader: Arc<AvailReader>,
    },
}

/// Owns persistent publication state and resumes incomplete jobs after restart.
#[derive(Clone)]
pub struct AvailabilityService {
    jobs: Tree,
    objects: Tree,
    input_retrievals: Tree,
    backend: Arc<Backend>,
    in_progress: Arc<Mutex<HashSet<String>>>,
    chain_id: u64,
    http_rpc_url: String,
    private_key: String,
    interfold_address: String,
    e3_program_address: String,
    ciphernode_registry_address: String,
    input_duration_seconds: u64,
    proof_lead_seconds: u64,
}

impl AvailabilityService {
    pub fn new(db: &Db, config: &Config) -> anyhow::Result<Self> {
        let mode = config.data_availability_mode();
        let backend = match mode.as_str() {
            "mock" => Backend::Mock,
            "avail" => {
                let rpc_url = config
                    .avail_rpc_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("AVAIL_RPC_URL is required"))?;
                Backend::Avail {
                    publisher: Arc::new(AvailPublisher::new(
                        rpc_url,
                        config
                            .avail_app_id
                            .ok_or_else(|| anyhow::anyhow!("AVAIL_APP_ID is required"))?,
                        config
                            .avail_seed
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("AVAIL_SEED is required"))?,
                        config
                            .avail_bridge_api_url
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("AVAIL_BRIDGE_API_URL is required"))?,
                        config.chain_id,
                    )?),
                    reader: Arc::new(AvailReader::new(rpc_url)?),
                }
            }
            other => anyhow::bail!("unsupported DATA_AVAILABILITY_MODE '{other}'"),
        };
        Ok(Self {
            jobs: db.open_tree("data-availability-jobs")?,
            objects: db.open_tree("data-availability-objects")?,
            input_retrievals: db.open_tree("data-availability-input-retrievals")?,
            backend: Arc::new(backend),
            in_progress: Arc::new(Mutex::new(HashSet::new())),
            chain_id: config.chain_id,
            http_rpc_url: config.http_rpc_url.clone(),
            private_key: config.private_key.clone(),
            interfold_address: config.interfold_address.clone(),
            e3_program_address: config.e3_program_address.clone(),
            ciphernode_registry_address: config.ciphernode_registry_address.clone(),
            input_duration_seconds: config.e3_duration,
            proof_lead_seconds: config.avail_proof_lead_seconds.unwrap_or(10_800),
        })
    }

    /// Check local timing against the current registry, Interfold, and CRISP contract values.
    pub async fn validate_onchain_configuration(&self) -> anyhow::Result<()> {
        if !matches!(&*self.backend, Backend::Avail { .. }) {
            return Ok(());
        }
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let onchain = duration_u64(
            contract
                .availability_finalization_window()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            "CRISP finalization window",
        )?;
        anyhow::ensure!(
            onchain == self.proof_lead_seconds,
            "AVAIL_PROOF_LEAD_SECONDS ({}) does not match CRISPProgram.availabilityFinalizationWindow() ({onchain})",
            self.proof_lead_seconds
        );

        let registry: Address = self
            .ciphernode_registry_address
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid ciphernode registry address: {error}"))?;
        let (randomness, sortition) = contract
            .committee_setup_windows(registry)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let interfold =
            InterfoldContractFactory::create_read(&self.http_rpc_url, &self.interfold_address)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let timeouts = interfold
            .get_timeout_config()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let voting = contract
            .minimum_voting_duration()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let required = minimum_input_duration(
            duration_u64(randomness, "randomness request timeout")?,
            duration_u64(sortition, "sortition submission window")?,
            duration_u64(timeouts.dkgWindow, "DKG window")?,
            duration_u64(voting, "minimum voting duration")?,
            onchain,
        )?;
        anyhow::ensure!(
            self.input_duration_seconds >= required,
            "E3_DURATION ({}) is shorter than the current on-chain committee, voting, and availability windows ({required})",
            self.input_duration_seconds
        );
        Ok(())
    }

    pub async fn stage_input(
        &self,
        e3_id: &str,
        encoded_envelope: Vec<u8>,
    ) -> anyhow::Result<AvailabilityJobView> {
        let envelope = decode_input_envelope(&encoded_envelope)
            .map_err(|_| reject_input("The encoded vote envelope is invalid"))?;
        e3_data_availability::validate_object_bytes(&envelope.availabilityProof)
            .map_err(|_| reject_input("The encrypted vote is too large"))?;
        let actual = keccak256(&envelope.availabilityProof);
        if actual != envelope.encryptedVoteHash {
            return Err(reject_input(
                "The encrypted vote does not match its committed hash",
            ));
        }

        // A proof system can produce more than one valid proof for the same public statement.
        // Keep the durable job keyed by that statement, not by the proof bytes, or retrying with
        // another valid proof can buy the same Avail publication twice.
        let request_identity = (
            envelope.slotAddress,
            envelope.encryptedVoteCommitment,
            envelope.parentIndexPlusOne,
        )
            .abi_encode();
        let id = self.job_id(b"input", e3_id, actual, &request_identity);
        if self.load(&id)?.is_some() {
            self.process(&id).await;
            return Ok((&self.load_required(&id)?).into());
        }

        // Reject invalid Noir proofs before the service pays an Avail submission fee.
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        contract
            .validate_input_proof(
                e3_id_to_u256(e3_id)
                    .map_err(|_| reject_input("The E3 identifier is invalid"))?,
                envelope.noirProof.clone(),
                envelope.slotAddress,
                envelope.encryptedVoteCommitment,
                envelope.encryptedVoteHash,
                envelope.parentIndexPlusOne.to::<u64>(),
            )
            .await?;

        let (deadline, commitment_deadline) = if matches!(&*self.backend, Backend::Avail { .. }) {
            let interfold =
                InterfoldContractFactory::create_read(&self.http_rpc_url, &self.interfold_address)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let e3_id_value = e3_id_to_u256(e3_id)
                .map_err(|_| reject_input("The E3 identifier is invalid"))?;
            let e3 = interfold
                .get_e3(e3_id_value)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let now = self.chain_timestamp().await?;
            let input_deadline: u64 = e3.inputWindow[1]
                .try_into()
                .map_err(|_| anyhow::anyhow!("input deadline does not fit in u64"))?;
            let deadline: u64 = interfold
                .get_deadlines(e3_id_value)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .computeDeadline
                .try_into()
                .map_err(|_| anyhow::anyhow!("compute deadline does not fit in u64"))?;
            let commitment_deadline = contract
                .input_commitment_deadline(e3_id_value)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            if commitment_deadline <= now {
                return Err(reject_input("The vote commitment deadline has passed"));
            }
            anyhow::ensure!(
                input_deadline.saturating_sub(commitment_deadline) >= self.proof_lead_seconds,
                "the CRISP finalization tail is shorter than AVAIL_PROOF_LEAD_SECONDS"
            );
            (deadline, commitment_deadline)
        } else {
            (no_deadline(), no_deadline())
        };

        let job = AvailabilityJob {
            id: id.clone(),
            content_hash: actual.0,
            object: envelope.availabilityProof.to_vec(),
            kind: JobKind::Input {
                e3_id: e3_id.to_owned(),
                staged_envelope: encoded_envelope,
                deadline,
                commitment_deadline,
            },
            state: JobState::Created,
        };
        // Persist the bytes before an attestation can be returned. The signature promises that
        // this service received the exact object and can resume its job after a restart.
        self.objects.insert(actual.0, job.object.as_slice())?;
        self.objects.flush()?;
        self.save(&job)?;
        self.process(&id).await;
        if matches!(&*self.backend, Backend::Mock) {
            // Local mode has no external finality delay. Drive every durable phase so callers
            // keep the synchronous developer experience while production remains asynchronous.
            self.process(&id).await;
            self.process(&id).await;
            self.process(&id).await;
        }
        Ok((&self.load_required(&id)?).into())
    }

    pub async fn stage_output(
        &self,
        e3_id: &str,
        ciphertext: Vec<u8>,
        ciphertext_commitment: [u8; 32],
        compute_proof: Vec<u8>,
    ) -> anyhow::Result<AvailabilityJobView> {
        e3_data_availability::validate_object_bytes(&ciphertext)?;
        let hash = keccak256(&ciphertext);
        // The output statement is the E3, exact ciphertext hash, and ciphertext commitment. The
        // RISC Zero seal proves that statement but is not its identity: another valid seal must be
        // an idempotent retry, not another paid Avail publication.
        let id = self.job_id(b"output", e3_id, hash, &ciphertext_commitment);
        if let Some(job) = self.load(&id)? {
            return Ok((&job).into());
        }
        let deadline = if matches!(&*self.backend, Backend::Avail { .. }) {
            let interfold =
                InterfoldContractFactory::create_read(&self.http_rpc_url, &self.interfold_address)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let e3_id_value = e3_id_to_u256(e3_id)?;
            anyhow::ensure!(
                interfold
                    .get_e3_stage(e3_id_value)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?
                    == E3Stage::KeyPublished,
                "the E3 is not accepting an aggregate ciphertext"
            );
            let e3 = interfold
                .get_e3(e3_id_value)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let deadlines = interfold
                .get_deadlines(e3_id_value)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let deadline: u64 = deadlines
                .computeDeadline
                .try_into()
                .map_err(|_| anyhow::anyhow!("compute deadline does not fit in u64"))?;
            let now = self.chain_timestamp().await?;
            let input_deadline: u64 = e3.inputWindow[1]
                .try_into()
                .map_err(|_| anyhow::anyhow!("input deadline does not fit in u64"))?;
            anyhow::ensure!(
                now >= input_deadline,
                "the input window is still open; the aggregate proof could become stale"
            );
            anyhow::ensure!(
                deadline > now.saturating_add(self.proof_lead_seconds),
                "the compute deadline arrives before VectorX can safely prove this publication"
            );
            deadline
        } else {
            no_deadline()
        };

        // `/state/add-result` is reachable over HTTP. Do not let an arbitrary caller spend the
        // Avail signer balance: first execute the exact CRISP proof check that Interfold will use
        // once the VectorX receipt exists. Invalid output never becomes durable work.
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        contract
            .validate_compute_output(
                e3_id_to_u256(e3_id)?,
                hash,
                B256::from(ciphertext_commitment),
                Bytes::copy_from_slice(&compute_proof),
            )
            .await
            .map_err(|error| {
                anyhow::anyhow!("the aggregate ciphertext proof is not acceptable: {error}")
            })?;

        let job = AvailabilityJob {
            id: id.clone(),
            content_hash: hash.0,
            object: ciphertext,
            kind: JobKind::Output {
                e3_id: e3_id.to_owned(),
                ciphertext_commitment,
                compute_proof,
                deadline,
            },
            state: JobState::Created,
        };
        self.save(&job)?;
        if matches!(&*self.backend, Backend::Mock) {
            self.process(&id).await;
            self.process(&id).await;
        }
        Ok((&self.load_required(&id)?).into())
    }

    /// Read a job after reconciling wallet-submitted work with Ethereum.
    ///
    /// A browser can close after its input commitment is mined but before the background worker
    /// observes it. On reload, returning the cached `AwaitingCommitment` state would offer the same
    /// transaction again. This bounded read checks the one relevant on-chain fact first. A slow RPC
    /// does not make the status endpoint unavailable; the durable worker still retries normally.
    pub async fn refreshed_view(&self, id: &str) -> anyhow::Result<Option<AvailabilityJobView>> {
        let Some(mut job) = self.load(id)? else {
            return Ok(None);
        };
        if matches!(
            &job.state,
            JobState::Submitted { .. } | JobState::Failed { .. }
        ) {
            return Ok(Some((&job).into()));
        }

        let refresh = async {
            if matches!(&job.state, JobState::AwaitingCommitment { .. }) {
                if self.input_is_committed(&job).await? {
                    job.state = JobState::Committed {
                        transaction_hash: "wallet-committed".to_owned(),
                    };
                    self.save(&job)?;
                }
            } else if self.ethereum_publication_exists(&job).await? {
                job.state = JobState::Submitted {
                    transaction_hash: "already-finalized".to_owned(),
                };
                self.save(&job)?;
            }
            anyhow::Ok(())
        };

        match tokio::time::timeout(JOB_STATUS_REFRESH_TIMEOUT, refresh).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(job_id = id, %error, "Could not refresh availability job from Ethereum")
            }
            Err(_) => warn!(
                job_id = id,
                "Timed out while refreshing availability job from Ethereum"
            ),
        }

        Ok(Some((&job).into()))
    }

    pub fn object(&self, hash: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let hash = hash.strip_prefix("0x").unwrap_or(hash);
        let key = hex::decode(hash)?;
        Ok(self.objects.get(key)?.map(|value| value.to_vec()))
    }

    /// Retrieve bytes named by a receipt that the Ethereum contract already accepted.
    pub async fn retrieve(&self, reference: DataReference) -> anyhow::Result<Vec<u8>> {
        if let Some(bytes) = self
            .objects
            .get(reference.content_hash)?
            .map(|value| value.to_vec())
        {
            return e3_data_availability::verify_retrieved_bytes(reference, bytes);
        }

        let bytes = match &*self.backend {
            Backend::Mock => anyhow::bail!(
                "local data-availability object 0x{} is not stored",
                hex::encode(reference.content_hash)
            ),
            Backend::Avail { reader, .. } => reader.retrieve(reference).await?,
        };
        self.objects
            .insert(reference.content_hash, bytes.as_slice())?;
        self.objects.flush()?;
        Ok(bytes)
    }

    pub fn record_input_reference(
        &self,
        reference: &AvailableInputReference,
    ) -> anyhow::Result<()> {
        self.input_retrievals
            .insert(reference.key(), serde_json::to_vec(reference)?)?;
        self.input_retrievals.flush()?;
        Ok(())
    }

    pub fn pending_input_references(&self) -> Vec<AvailableInputReference> {
        self.input_retrievals
            .iter()
            .filter_map(|entry| {
                let (_, value) = entry.ok()?;
                serde_json::from_slice(&value).ok()
            })
            .collect()
    }

    pub fn complete_input_reference(
        &self,
        reference: &AvailableInputReference,
    ) -> anyhow::Result<()> {
        self.input_retrievals.remove(reference.key())?;
        self.input_retrievals.flush()?;
        Ok(())
    }

    pub async fn run(self: Arc<Self>) {
        loop {
            let ids = self.pending_ids();
            for id in ids {
                let service = Arc::clone(&self);
                tokio::spawn(async move {
                    service.process(&id).await;
                });
            }
            tokio::time::sleep(JOB_POLL_INTERVAL).await;
        }
    }

    async fn process(&self, id: &str) {
        {
            let mut active = self.in_progress.lock().await;
            if !active.insert(id.to_owned()) {
                return;
            }
        }
        match tokio::time::timeout(JOB_STEP_TIMEOUT, self.process_inner(id)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!(job_id = id, %error, "Data-availability job will retry"),
            Err(_) => warn!(
                job_id = id,
                "Data-availability job step timed out and will retry"
            ),
        }
        self.in_progress.lock().await.remove(id);
    }

    async fn process_inner(&self, id: &str) -> anyhow::Result<()> {
        let mut job = self.load_required(id)?;
        let terminal = matches!(
            &job.state,
            JobState::Submitted { .. } | JobState::Failed { .. }
        );
        if terminal {
            return Ok(());
        }
        if !terminal && self.ethereum_publication_exists(&job).await? {
            job.state = JobState::Submitted {
                transaction_hash: "already-finalized".to_owned(),
            };
            self.save(&job)?;
            return Ok(());
        }
        let now = self.chain_timestamp().await?;
        if matches!(&*self.backend, Backend::Avail { .. }) && now > job.kind.deadline() {
            // A load-balanced RPC can expose a new head while serving contract state from an
            // older one. Do not strand a publication that landed at the deadline on that stale
            // read. Once a finalized block after the deadline still lacks it, no later block can
            // accept it and the failure is conclusive.
            if let Some(block) = self
                .finalized_block_past(job.kind.deadline(), false)
                .await?
            {
                if self.ethereum_publication_exists_at(&job, block).await? {
                    job.state = JobState::Submitted {
                        transaction_hash: "already-finalized".to_owned(),
                    };
                } else {
                    job.state = JobState::Failed {
                        message:
                            "the Ethereum publication deadline passed before the availability job completed"
                                .to_owned(),
                    };
                }
                self.save(&job)?;
            }
            return Ok(());
        }

        if let JobKind::Input {
            commitment_deadline,
            ..
        } = &job.kind
        {
            let waiting_for_commitment = matches!(
                &job.state,
                JobState::Created | JobState::AwaitingCommitment { .. }
            );
            if waiting_for_commitment
                && now >= *commitment_deadline
                && !self.input_is_committed(&job).await?
            {
                // The cutoff itself is exclusive. A finalized block at or after it contains every
                // commitment that could still have succeeded. Use that historical state rather
                // than a possibly stale latest-state read.
                if let Some(block) = self
                    .finalized_block_past(*commitment_deadline, true)
                    .await?
                {
                    if self.input_is_committed_at(&job, block).await? {
                        job.state = JobState::Committed {
                            transaction_hash: "wallet-committed".to_owned(),
                        };
                    } else {
                        job.state = JobState::Failed {
                            message:
                                "the input proof commitment deadline passed before Ethereum accepted it"
                                    .to_owned(),
                        };
                    }
                    self.save(&job)?;
                }
                return Ok(());
            }

            // Jobs created by the first Avail prototype may already hold a publication or proof
            // without the new Ethereum commitment. Preserve that paid work on Sepolia/local by
            // inserting the missing first step before the job continues.
            if matches!(
                &job.state,
                JobState::AwaitingProof { .. } | JobState::Ready { .. }
            ) && !self.input_is_committed(&job).await?
            {
                anyhow::ensure!(
                    self.chain_id != 1,
                    "an old mainnet availability job has no input commitment; restage it"
                );
                let receipt = self.submit_input_commitment(&job).await?;
                let hash = receipt.transaction_hash.to_string();
                match &mut job.state {
                    JobState::AwaitingProof {
                        commitment_transaction_hash,
                        ..
                    }
                    | JobState::Ready {
                        commitment_transaction_hash,
                        ..
                    } => *commitment_transaction_hash = Some(hash),
                    _ => unreachable!("state was checked above"),
                }
                self.save(&job)?;
                return Ok(());
            }
        }

        match job.state.clone() {
            JobState::Created => {
                match &job.kind {
                    JobKind::Input { .. } if self.input_is_committed(&job).await? => {
                        job.state = JobState::Committed {
                            transaction_hash: "already-committed".to_owned(),
                        };
                    }
                    JobKind::Input { .. } if self.chain_id == 1 => {
                        job.state = JobState::AwaitingCommitment {
                            ethereum_payload: self.commitment_payload(&job).await?,
                        };
                    }
                    JobKind::Input { .. } => {
                        let receipt = self.submit_input_commitment(&job).await?;
                        job.state = JobState::Committed {
                            transaction_hash: receipt.transaction_hash.to_string(),
                        };
                    }
                    JobKind::Output { .. } => {
                        job.state = self.start_availability(&job, None).await?;
                    }
                }
                self.save(&job)?;
            }
            JobState::AwaitingCommitment { .. } => {
                if self.input_is_committed(&job).await? {
                    job.state = JobState::Committed {
                        transaction_hash: "wallet-committed".to_owned(),
                    };
                    self.save(&job)?;
                }
            }
            JobState::Committed { transaction_hash } => {
                job.state = self
                    .start_availability(&job, Some(transaction_hash))
                    .await?;
                self.save(&job)?;
            }
            JobState::AwaitingProof {
                publication,
                commitment_transaction_hash,
            } => {
                let Backend::Avail { publisher, .. } = &*self.backend else {
                    anyhow::bail!("mock job cannot await a VectorX proof");
                };
                if let ProofStatus::Ready { abi_proof, .. } = publisher.proof(&publication).await? {
                    job.state = JobState::Ready {
                        ethereum_payload: abi_proof,
                        commitment_transaction_hash,
                    };
                    self.save(&job)?;
                }
            }
            JobState::Ready {
                ethereum_payload, ..
            } => match &job.kind {
                JobKind::Input { .. } => {
                    anyhow::ensure!(
                        self.input_is_committed(&job).await?,
                        "cannot finalize an input whose proof commitment is absent"
                    );
                    let receipt = self.finalize_input(&job, &ethereum_payload).await?;
                    job.state = JobState::Submitted {
                        transaction_hash: receipt.transaction_hash.to_string(),
                    };
                    self.save(&job)?;
                }
                JobKind::Output {
                    e3_id,
                    ciphertext_commitment,
                    compute_proof,
                    ..
                } => {
                    let contract = InterfoldContractFactory::create_write(
                        &self.http_rpc_url,
                        &self.interfold_address,
                        &self.private_key,
                    )
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    let e3_id = e3_id_to_u256(e3_id)?;
                    let stage = contract
                        .get_e3_stage(e3_id)
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    match stage {
                        E3Stage::KeyPublished => {}
                        E3Stage::CiphertextReady | E3Stage::Complete => {
                            job.state = JobState::Submitted {
                                transaction_hash: "already-finalized".to_owned(),
                            };
                            self.save(&job)?;
                            return Ok(());
                        }
                        E3Stage::Failed => {
                            job.state = JobState::Failed {
                                message:
                                    "the E3 failed before its aggregate ciphertext was published"
                                        .to_owned(),
                            };
                            self.save(&job)?;
                            return Ok(());
                        }
                        E3Stage::None | E3Stage::Requested | E3Stage::CommitteeFinalized => {
                            anyhow::bail!("the E3 is not ready for its aggregate ciphertext");
                        }
                        stage => {
                            anyhow::bail!(
                                    "unsupported E3 stage {stage:?} while publishing an aggregate ciphertext"
                                );
                        }
                    }
                    let receipt = contract
                        .publish_ciphertext_output(
                            e3_id,
                            B256::from(job.content_hash),
                            B256::from(*ciphertext_commitment),
                            Bytes::copy_from_slice(compute_proof),
                            Bytes::copy_from_slice(&ethereum_payload),
                        )
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    job.state = JobState::Submitted {
                        transaction_hash: receipt.transaction_hash.to_string(),
                    };
                    self.save(&job)?;
                }
            },
            JobState::Submitted { .. } | JobState::Failed { .. } => unreachable!(),
        }
        Ok(())
    }

    async fn start_availability(
        &self,
        job: &AvailabilityJob,
        commitment_transaction_hash: Option<String>,
    ) -> anyhow::Result<JobState> {
        match &*self.backend {
            Backend::Mock => {
                self.objects
                    .insert(job.content_hash, job.object.as_slice())?;
                self.objects.flush()?;
                Ok(JobState::Ready {
                    ethereum_payload: job.object.clone(),
                    commitment_transaction_hash,
                })
            }
            Backend::Avail { publisher, .. } => {
                let publication = publisher.publish(&job.object).await?;
                anyhow::ensure!(
                    publication.content_hash == job.content_hash,
                    "Avail returned a different content hash"
                );
                Ok(JobState::AwaitingProof {
                    publication,
                    commitment_transaction_hash,
                })
            }
        }
    }

    async fn commitment_payload(&self, job: &AvailabilityJob) -> anyhow::Result<Vec<u8>> {
        let JobKind::Input {
            e3_id,
            staged_envelope,
            ..
        } = &job.kind
        else {
            anyhow::bail!("aggregate ciphertext jobs have no input commitment payload");
        };
        let envelope = decode_input_envelope(staged_envelope)?;
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let signer: PrivateKeySigner = self
            .private_key
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid availability signer key: {error}"))?;
        let configured = contract
            .input_availability_signer()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        anyhow::ensure!(
            configured == signer.address(),
            "the CRISP inputAvailabilitySigner does not match this service key"
        );
        let digest = contract
            .input_availability_digest(
                e3_id_to_u256(e3_id)?,
                envelope.encryptedVoteHash,
                envelope.encryptedVoteCommitment,
                envelope.slotAddress,
                envelope.parentIndexPlusOne.to::<u64>(),
            )
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let attestation = signer
            .sign_hash_sync(&digest)
            .map_err(|error| anyhow::anyhow!("failed to attest input availability: {error}"))?;
        let commitment_envelope = InputCommitmentEnvelope {
            noirProof: envelope.noirProof,
            slotAddress: envelope.slotAddress,
            encryptedVoteCommitment: envelope.encryptedVoteCommitment,
            encryptedVoteHash: envelope.encryptedVoteHash,
            parentIndexPlusOne: envelope.parentIndexPlusOne,
            availabilityAttestation: Bytes::copy_from_slice(&attestation.as_bytes()),
        };
        Ok(encode_input_commitment_envelope(&commitment_envelope))
    }

    async fn submit_input_commitment(
        &self,
        job: &AvailabilityJob,
    ) -> anyhow::Result<alloy::rpc::types::TransactionReceipt> {
        let JobKind::Input { e3_id, .. } = &job.kind else {
            anyhow::bail!("aggregate ciphertext jobs cannot commit an input");
        };
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let e3_id = e3_id_to_u256(e3_id)?;
        let payload = Bytes::from(self.commitment_payload(job).await?);
        contract
            .simulate_publish_input(e3_id, payload.clone())
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        contract
            .publish_input(e3_id, payload)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn finalize_input(
        &self,
        job: &AvailabilityJob,
        availability_proof: &[u8],
    ) -> anyhow::Result<alloy::rpc::types::TransactionReceipt> {
        let JobKind::Input {
            e3_id,
            staged_envelope,
            ..
        } = &job.kind
        else {
            anyhow::bail!("aggregate ciphertext jobs cannot finalize an input");
        };
        let envelope = decode_input_envelope(staged_envelope)?;
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let e3_id = e3_id_to_u256(e3_id)?;
        let availability_proof = Bytes::copy_from_slice(availability_proof);
        contract
            .simulate_finalize_input(
                e3_id,
                envelope.slotAddress,
                envelope.encryptedVoteCommitment,
                envelope.encryptedVoteHash,
                envelope.parentIndexPlusOne.to::<u64>(),
                availability_proof.clone(),
            )
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        contract
            .finalize_input(
                e3_id,
                envelope.slotAddress,
                envelope.encryptedVoteCommitment,
                envelope.encryptedVoteHash,
                envelope.parentIndexPlusOne.to::<u64>(),
                availability_proof,
            )
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    fn job_id(
        &self,
        domain: &[u8],
        e3_id: &str,
        content_hash: B256,
        request_identity: &[u8],
    ) -> String {
        let mut identity = Vec::with_capacity(domain.len() + e3_id.len() + 64);
        identity.extend_from_slice(domain);
        identity.extend_from_slice(e3_id.as_bytes());
        identity.extend_from_slice(content_hash.as_slice());
        identity.extend_from_slice(keccak256(request_identity).as_slice());
        format!("0x{}", hex::encode(keccak256(identity)))
    }

    async fn chain_timestamp(&self) -> anyhow::Result<u64> {
        let block = tokio::time::timeout(Duration::from_secs(15), async {
            let provider = ProviderBuilder::new().connect(&self.http_rpc_url).await?;
            provider.get_block_by_number(BlockNumberOrTag::Latest).await
        })
        .await
        .map_err(|_| anyhow::anyhow!("timed out while reading the Ethereum head"))??
        .ok_or_else(|| anyhow::anyhow!("the Ethereum RPC returned no latest block"))?;
        Ok(block.header.timestamp)
    }

    /// Return a finalized block that proves a deadline has passed.
    ///
    /// Commitment is rejected at its exact cutoff, so `inclusive` accepts a finalized block at
    /// that timestamp. Input and output finalization are valid through their exact deadline, so
    /// those decisions require a strictly later finalized block.
    async fn finalized_block_past(
        &self,
        deadline: u64,
        inclusive: bool,
    ) -> anyhow::Result<Option<u64>> {
        let block = tokio::time::timeout(Duration::from_secs(15), async {
            let provider = ProviderBuilder::new().connect(&self.http_rpc_url).await?;
            provider
                .get_block_by_number(BlockNumberOrTag::Finalized)
                .await
        })
        .await
        .map_err(|_| anyhow::anyhow!("timed out while reading the finalized Ethereum head"))??
        .ok_or_else(|| anyhow::anyhow!("the Ethereum RPC returned no finalized block"))?;
        let passed = if inclusive {
            block.header.timestamp >= deadline
        } else {
            block.header.timestamp > deadline
        };
        Ok(passed.then_some(block.header.number))
    }

    async fn input_is_committed_at(
        &self,
        job: &AvailabilityJob,
        block_number: u64,
    ) -> anyhow::Result<bool> {
        let JobKind::Input {
            e3_id,
            staged_envelope,
            ..
        } = &job.kind
        else {
            return Ok(false);
        };
        let envelope = decode_input_envelope(staged_envelope)?;
        let provider = ProviderBuilder::new().connect(&self.http_rpc_url).await?;
        let contract = ICrispAvailabilityState::new(self.e3_program_address.parse()?, provider);
        Ok(contract
            .isInputCommitted(
                e3_id_to_u256(e3_id)?,
                envelope.encryptedVoteHash,
                envelope.encryptedVoteCommitment,
                envelope.slotAddress,
                envelope.parentIndexPlusOne,
            )
            .block(BlockId::number(block_number))
            .call()
            .await?)
    }

    async fn ethereum_publication_exists_at(
        &self,
        job: &AvailabilityJob,
        block_number: u64,
    ) -> anyhow::Result<bool> {
        let provider = ProviderBuilder::new().connect(&self.http_rpc_url).await?;
        match &job.kind {
            JobKind::Input {
                e3_id,
                staged_envelope,
                ..
            } => {
                let envelope = decode_input_envelope(staged_envelope)?;
                let contract =
                    ICrispAvailabilityState::new(self.e3_program_address.parse()?, provider);
                Ok(contract
                    .isInputPublished(
                        e3_id_to_u256(e3_id)?,
                        envelope.encryptedVoteHash,
                        envelope.encryptedVoteCommitment,
                        envelope.slotAddress,
                        envelope.parentIndexPlusOne,
                    )
                    .block(BlockId::number(block_number))
                    .call()
                    .await?)
            }
            JobKind::Output { e3_id, .. } => {
                let contract =
                    IInterfoldAvailabilityState::new(self.interfold_address.parse()?, provider);
                let stage = contract
                    .getE3Stage(e3_id_to_u256(e3_id)?)
                    .block(BlockId::number(block_number))
                    .call()
                    .await?;
                Ok(matches!(
                    stage,
                    StoredE3Stage::CiphertextReady | StoredE3Stage::Complete
                ))
            }
        }
    }

    async fn input_is_published(&self, job: &AvailabilityJob) -> anyhow::Result<bool> {
        let JobKind::Input {
            e3_id,
            staged_envelope,
            ..
        } = &job.kind
        else {
            return Ok(false);
        };
        let envelope = decode_input_envelope(staged_envelope)?;
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        contract
            .is_input_published(
                e3_id_to_u256(e3_id)?,
                envelope.encryptedVoteHash,
                envelope.encryptedVoteCommitment,
                envelope.slotAddress,
                envelope.parentIndexPlusOne.to::<u64>(),
            )
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn input_is_committed(&self, job: &AvailabilityJob) -> anyhow::Result<bool> {
        let JobKind::Input {
            e3_id,
            staged_envelope,
            ..
        } = &job.kind
        else {
            return Ok(false);
        };
        let envelope = decode_input_envelope(staged_envelope)?;
        let contract = CRISPContract::new(
            &self.http_rpc_url,
            &self.private_key,
            &self.e3_program_address,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        contract
            .is_input_committed(
                e3_id_to_u256(e3_id)?,
                envelope.encryptedVoteHash,
                envelope.encryptedVoteCommitment,
                envelope.slotAddress,
                envelope.parentIndexPlusOne.to::<u64>(),
            )
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn ethereum_publication_exists(&self, job: &AvailabilityJob) -> anyhow::Result<bool> {
        match &job.kind {
            JobKind::Input { .. } => self.input_is_published(job).await,
            JobKind::Output { e3_id, .. } => {
                let contract = InterfoldContractFactory::create_read(
                    &self.http_rpc_url,
                    &self.interfold_address,
                )
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let stage = contract
                    .get_e3_stage(e3_id_to_u256(e3_id)?)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                Ok(matches!(
                    stage,
                    E3Stage::CiphertextReady | E3Stage::Complete
                ))
            }
        }
    }

    fn pending_ids(&self) -> Vec<String> {
        self.jobs
            .iter()
            .filter_map(|entry| {
                let (_, value) = entry.ok()?;
                let job: AvailabilityJob = serde_json::from_slice(&value).ok()?;
                let terminal = matches!(
                    &job.state,
                    JobState::Submitted { .. } | JobState::Failed { .. }
                );
                (!terminal).then_some(job.id)
            })
            .collect()
    }

    fn load(&self, id: &str) -> anyhow::Result<Option<AvailabilityJob>> {
        self.jobs
            .get(id.as_bytes())?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
            .transpose()
    }

    fn load_required(&self, id: &str) -> anyhow::Result<AvailabilityJob> {
        self.load(id)?
            .ok_or_else(|| anyhow::anyhow!("data-availability job {id} does not exist"))
    }

    fn save(&self, job: &AvailabilityJob) -> anyhow::Result<()> {
        self.jobs
            .insert(job.id.as_bytes(), serde_json::to_vec(job)?)?;
        self.jobs.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDK_INPUT_ENVELOPE: &str = concat!(
        "00000000000000000000000000000000000000000000000000000000000000c0",
        "0000000000000000000000001111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222222222222222222222222222",
        "3333333333333333333333333333333333333333333333333333333333333333",
        "0000000000000000000000000000000000000000000000000000000000000007",
        "0000000000000000000000000000000000000000000000000000000000000100",
        "0000000000000000000000000000000000000000000000000000000000000003",
        "0102030000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000002",
        "aabb000000000000000000000000000000000000000000000000000000000000",
    );

    #[test]
    fn sdk_input_envelope_uses_solidity_parameter_encoding() {
        let encoded = hex::decode(SDK_INPUT_ENVELOPE).unwrap();
        let envelope = decode_input_envelope(&encoded).unwrap();

        assert_eq!(envelope.noirProof.as_ref(), &[1, 2, 3]);
        assert_eq!(
            envelope.slotAddress,
            "0x1111111111111111111111111111111111111111"
                .parse::<alloy::primitives::Address>()
                .unwrap()
        );
        assert_eq!(envelope.encryptedVoteCommitment, B256::repeat_byte(0x22));
        assert_eq!(envelope.encryptedVoteHash, B256::repeat_byte(0x33));
        assert_eq!(envelope.parentIndexPlusOne.to::<u64>(), 7);
        assert_eq!(envelope.availabilityProof.as_ref(), &[0xaa, 0xbb]);

        let commitment_envelope = InputCommitmentEnvelope {
            noirProof: envelope.noirProof,
            slotAddress: envelope.slotAddress,
            encryptedVoteCommitment: envelope.encryptedVoteCommitment,
            encryptedVoteHash: envelope.encryptedVoteHash,
            parentIndexPlusOne: envelope.parentIndexPlusOne,
            availabilityAttestation: envelope.availabilityProof,
        };
        assert_eq!(
            encode_input_commitment_envelope(&commitment_envelope),
            encoded
        );
    }

    #[test]
    fn only_conclusive_input_errors_have_a_client_rejection_message() {
        let malformed = reject_input("The encoded vote envelope is invalid");
        assert_eq!(
            input_rejection_message(&malformed),
            Some("The encoded vote envelope is invalid")
        );

        let reverted = anyhow::Error::new(SimulateError::Reverted("node detail".to_owned()));
        assert_eq!(
            input_rejection_message(&reverted),
            Some("The vote proof or ciphertext was rejected")
        );

        let provider = anyhow::Error::new(SimulateError::Provider("secret RPC detail".to_owned()));
        assert_eq!(input_rejection_message(&provider), None);
    }

    #[test]
    fn input_duration_adds_current_onchain_windows() {
        assert_eq!(
            minimum_input_duration(1_200, 300, 3_600, 3_600, 10_800).unwrap(),
            19_500
        );
        assert!(minimum_input_duration(u64::MAX, 1, 0, 0, 0).is_err());
    }
}
