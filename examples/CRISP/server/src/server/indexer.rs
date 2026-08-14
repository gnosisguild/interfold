// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::server::token_holders::{
    get_mock_token_holders, try_fetch_requester_census, EtherscanClient,
};
use crate::server::{
    models::{CensusMode, CreditMode, CurrentRound, CustomParams, TokenHolder},
    program_server_request::run_compute,
    repo::{CrispE3Repository, CurrentRoundRepository},
    token_holders::{build_tree, compute_token_holder_hashes},
    CONFIG,
};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol_types::{sol_data, SolType};
use alloy_primitives::{Address, U256};
use crisp_utils::decode_tally;
use e3_sdk::{
    evm_helpers::{
        contracts::{InterfoldRead, ReadWrite},
        events::{
            CiphertextOutputPublished, CommitteePublished, E3Requested, PlaintextOutputPublished,
        },
        retry::call_with_retry,
    },
    indexer::{DataStore, InterfoldIndexer, SharedStore},
};
use evm_helpers::{CRISPContractFactory, InputPublished};
use eyre::Context;
use log::{info, warn};
use num_bigint::BigUint;
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub async fn register_e3_requested(
    indexer: InterfoldIndexer<impl DataStore, ReadWrite>,
) -> Result<InterfoldIndexer<impl DataStore, ReadWrite>> {
    // E3Requested
    indexer
        .add_event_handler(move |event: E3Requested, ctx| {
            let store = ctx.store();
            let e3_id = event.e3Id.to_string();
            let mut repo = CrispE3Repository::new(store.clone(), &e3_id);

            let contract = ctx.contract();

            info!("[e3_id={}] E3Requested: {:?}", e3_id, event);

            async move {
                // 0xcd6f4a4f = E3DoesNotExist()
                let e3 = call_with_retry("get_e3", &["0xcd6f4a4f"], || {
                    let contract = contract.clone();
                    let event_e3_id = event.e3Id;
                    async move {
                        contract
                            .get_e3(event_e3_id)
                            .await
                            .map_err(|e| anyhow::anyhow!("{}", e))
                    }
                })
                .await
                .map_err(|e| eyre::eyre!("{}", e))?;

                // Use sol_data types instead of primitives
                // Seven fields. The seventh is the ONCHAIN voting-power divisor: the contract
                // scales raw token power by it before handing the value to the circuit, so a
                // six-field decode would fail outright on every round requested after that field
                // was added.
                type CustomParamsTuple = (
                    sol_data::Address,
                    sol_data::Uint<256>,
                    sol_data::Uint<256>,
                    sol_data::Uint<256>,
                    sol_data::Uint<256>,
                    sol_data::Uint<256>,
                    sol_data::Uint<256>,
                );

                let decoded = <CustomParamsTuple as SolType>::abi_decode(&event.e3.customParams)
                    .with_context(|| "Failed to decode custom params from E3 event")?;

                // `saturating_to` rather than `to`: these fields are attacker-chosen ABI data, and
                // `to::<u64>()` panics on a value above `u64::MAX`. Clamping lets the `TryFrom`
                // impls reject it as an unknown mode instead.
                let credit_mode = CreditMode::try_from(decoded.3.saturating_to::<u64>())?;
                let census_mode = CensusMode::try_from(decoded.5.saturating_to::<u64>())?;
                let credits = match credit_mode {
                    CreditMode::Constant => {
                        info!("[e3_id={}] Credit mode: Constant", e3_id);
                        Some(decoded.4.to_string())
                    }
                    CreditMode::Custom => {
                        info!("[e3_id={}] Credit mode: Custom", e3_id);
                        None
                    }
                };

                let credits_clone = credits.clone();

                let custom_params = CustomParams {
                    token_address: decoded.0.to_string(),
                    balance_threshold: decoded.1.to_string(),
                    num_options: decoded.2.to_string(),
                    credit_mode,
                    credits,
                    census_mode,
                    voting_power_divisor: decoded.6.to_string(),
                };

                let balance_threshold =
                    BigUint::parse_bytes(custom_params.balance_threshold.as_bytes(), 10)
                        .ok_or_else(|| eyre::eyre!("Invalid balance threshold"))?;
                let token_address: Address = custom_params
                    .token_address
                    .parse()
                    .with_context(|| "Invalid token address")?;

                let input_window = [e3.inputWindow[0].to::<u64>(), e3.inputWindow[1].to::<u64>()];

                // The census is built one tick before the request, as the request timepoint
                // itself is not final when the E3 is requested.
                //
                // `requestBlock` is a timestamp, not a block height — the ticket token runs
                // an EIP-6372 `mode=timestamp` clock, and `Interfold.request` assigns
                // `block.timestamp` to match the checkpoints it is compared against. The
                // name is historical.
                let snapshot_timepoint = event.e3.requestBlock.to::<u64>().saturating_sub(1);

                // An on-chain census has nothing for the coordinator to build. `CRISPProgram`
                // reads each voter's power with `getPastVotes` when the input is published, so
                // there is no holder list to enumerate and no root to post — `setMerkleRoot` is
                // not just unnecessary here, it is unused: `_eligibility` never reads it in this
                // mode. The round is still recorded, because the API serves its metadata.
                // An on-chain census is not an eligibility input: `_eligibility` reads each
                // voter's power with `getPastVotes` when the input is published and never looks at
                // `merkleRoot`. The holder list is still discovered and stored, because clients
                // need somewhere to draw mask targets from — a mask is written to someone else's
                // slot, so without a list of who holds power there is nobody to mask.
                //
                // The distinction matters for what a wrong list can do. For a Merkle round the
                // list *is* the electorate, so an omission disenfranchises. Here it is an index
                // over what the chain already decides, so an omission costs mask cover and nothing
                // else — it can never enfranchise anyone the contract would refuse.
                let is_onchain_census = custom_params.census_mode == CensusMode::Onchain;
                if is_onchain_census {
                    info!(
                        "[e3_id={}] CensusMode::Onchain — discovering holders for mask targets; \
                         no merkle root will be posted",
                        e3_id
                    );
                }

                // Get token holders from Etherscan API or mocked data.
                // Asked only when the round declared it. Probing every requester and falling back
                // on failure would turn a broken census provider into a token vote over the wrong
                // electorate, silently — the round would run, and nothing would error.
                //
                // Checked before the local-network branch because a declared census is exact on any
                // network, including a devnet where the mock holders would otherwise be substituted.
                // Wrapped so a discovery failure can be downgraded for ONCHAIN rounds below.
                // Etherscan being down carries no eligibility meaning there: the contract reads
                // power per input, so the only cost is mask cover.
                let discovery: eyre::Result<Vec<TokenHolder>> = async {
                Ok(if custom_params.census_mode == CensusMode::ByRequester {
                    let credits_str = match custom_params.credit_mode {
                        CreditMode::Constant => credits_clone
                            .clone()
                            .expect("credits must be set for Constant mode"),
                        // A requester-supplied census names *who* may vote, not how much each vote
                        // weighs, so it only has meaning when every voter carries the same credits.
                        // `CRISPProgram.validate` rejects this pairing on chain, so reaching it here
                        // means the round was requested against a different program.
                        CreditMode::Custom => {
                            return Err(eyre::eyre!(
                                "[e3_id={}] CensusMode::ByRequester requires \
                                 CreditMode::Constant; got Custom",
                                e3_id
                            ))
                        }
                    };

                    info!(
                        "[e3_id={}] Census mode: ByRequester; asking {}",
                        e3_id, e3.requester
                    );

                    let census =
                        try_fetch_requester_census(e3.requester, &e3_id, &CONFIG.http_rpc_url)
                            .await
                            .ok_or_else(|| {
                                eyre::eyre!(
                                    "[e3_id={}] Round declared CensusMode::ByRequester but \
                                     requester {} returned no census. Refusing to fall back to \
                                     token discovery, which would enfranchise the wrong voters.",
                                    e3_id,
                                    e3.requester
                                )
                            })?;

                    census
                        .into_iter()
                        .map(|address| TokenHolder {
                            address: address.to_string(),
                            balance: credits_str.clone(),
                        })
                        .collect()
                } else if matches!(CONFIG.chain_id, 31337 | 1337) {
                    info!(
                        "[e3_id={}] Using mocked token holders for local network (chain_id: {})",
                        e3_id, CONFIG.chain_id
                    );

                    get_mock_token_holders()
                } else {
                    info!(
                        "[e3_id={}] Using Etherscan API for network (chain_id: {})",
                        e3_id, CONFIG.chain_id
                    );

                    let etherscan_client =
                        EtherscanClient::new(CONFIG.etherscan_api_key.clone(), CONFIG.chain_id);

                    match custom_params.credit_mode {
                        CreditMode::Constant => {
                            let credits_str = credits_clone.expect("credits must be set for Constant mode");
                            let credits_u256: alloy_primitives::Uint<256, 4> = U256::from_str_radix(&credits_str, 10)
                            .map_err(|e| eyre::eyre!("Failed to parse credits: {}", e))?;

                            etherscan_client
                            .get_token_holders_with_constant_balance(
                                token_address,
                                snapshot_timepoint,
                                &CONFIG.http_rpc_url,
                                credits_u256
                            )
                            .await
                            .context("Etherscan token-holder discovery failed")?
                        }
                        CreditMode::Custom => {
                            etherscan_client
                            .get_token_holders_with_voting_power(
                                token_address,
                                snapshot_timepoint,
                                &CONFIG.http_rpc_url,
                                U256::from_str_radix(&balance_threshold.to_string(), 10).map_err(
                                    |e| {
                                        eyre::eyre!(
                                            "[e3_id={}] Failed to convert balance threshold to U256: {}",
                                            e3_id,
                                            e
                                        )
                                    },
                                )?,
                                // Honoured only for an on-chain census, mirroring the contract:
                                // `_initRound` records the divisor for ONCHAIN rounds and ignores
                                // the field otherwise, because a Merkle round's bound comes from
                                // the census leaf. Applying it there would scale the census by one
                                // factor while `_tallyScale()` reads the results back assuming
                                // another, and the tally would be wrong with nothing to show it.
                                if is_onchain_census {
                                    match U256::from_str_radix(
                                        &custom_params.voting_power_divisor,
                                        10,
                                    ) {
                                        Ok(d) if !d.is_zero() => Some(d),
                                        _ => None,
                                    }
                                } else {
                                    None
                                },
                            )
                            .await
                            .context("Etherscan token-holder discovery failed")?
                        }
                    }
                })
                }
                .await;

                // Fatal only where the list decides who may vote. For a Merkle round an empty
                // census means nobody can ever cast a ballot, so failing loudly is right. For an
                // on-chain census the list is an index over what the contract already decides:
                // failing here would skip `initialize_round`, leaving a perfectly votable round
                // unrecorded and invisible to every client, because discovery happened to come
                // back empty — a missing API key or a rate limit would be enough.

                let token_holders = match discovery {
                    Ok(holders) => holders,
                    Err(e) if is_onchain_census => {
                        warn!(
                            "[e3_id={}] CensusMode::Onchain holder discovery failed: {:#}. The \
                             round is still recorded and votable — eligibility is read from the \
                             token at publish time — but clients have no mask targets.",
                            e3_id, e
                        );
                        Vec::new()
                    }
                    Err(e) => return Err(e),
                };

                if token_holders.is_empty() {
                    if !is_onchain_census {
                        return Err(eyre::eyre!(
                            "[e3_id={}] No eligible token holders found for token address {}.",
                            e3_id,
                            token_address
                        ));
                    }

                    warn!(
                        "[e3_id={}] CensusMode::Onchain discovery found no holders for {}. The \
                         round is still recorded and votable — eligibility is read from the token \
                         at publish time — but clients have no mask targets to draw from.",
                        e3_id, token_address
                    );
                }

                // save the e3 details
                repo.initialize_round(
                    custom_params,
                    e3.requester.to_string(),
                    input_window[1],
                    snapshot_timepoint,
                )
                .await?;

                // Store eligible addresses in the repository.
                repo.set_eligible_addresses(token_holders.clone())
                    .await?;

                // Poseidon hashes exist to build the census tree, and an on-chain census has no
                // tree: `_eligibility` reads power from the token per input. The addresses are
                // stored above and that is all a client needs here — a mask is written to someone
                // else's slot, so it needs a list of who holds power, not a membership proof.
                let token_holder_hashes = if is_onchain_census {
                    Vec::new()
                } else {
                    let hashes = compute_token_holder_hashes(&token_holders)
                        .with_context(|| "Failed to compute token holder hashes")?;

                    repo.set_token_holder_hashes(hashes.clone()).await?;

                    hashes
                };

                CurrentRoundRepository::new(store.clone())
                    .record_round(&e3_id)
                    .await?;

                // Skipped for an on-chain census: `_eligibility` never reads `merkleRoot` in
                // that mode, so posting one would spend gas to publish a value nothing consults —
                // and would imply the list gates eligibility when it does not.
                if !is_onchain_census {
                    let tree =
                        build_tree(token_holder_hashes).with_context(|| "Failed to build tree")?;
                    let merkle_root = tree
                        .root()
                        .ok_or_else(|| eyre::eyre!("Failed to get merkle root from tree"))?;

                    info!("[e3_id={}] Merkle root: {}", e3_id, merkle_root);

                    // Convert merkle root from hex string to U256.
                    let merkle_root_bytes = hex::decode(&merkle_root)
                        .with_context(|| format!("[e3_id={}] Merkle root is not valid hex", e3_id))?;
                    let merkle_root_u256 = U256::from_be_slice(&merkle_root_bytes);

                    let e3_id_u256 = U256::from_str_radix(&e3_id, 10)
                        .with_context(|| format!("[e3_id={}] Invalid E3 ID", e3_id))?;

                    info!(
                        "[e3_id={}] Calling setMerkleRoot with root: {}",
                        e3_id, merkle_root_u256
                    );

                    let contract = CRISPContractFactory::create_write(
                        &CONFIG.http_rpc_url,
                        &CONFIG.e3_program_address,
                        &CONFIG.private_key,
                    )
                    .await
                    .with_context(|| {
                        format!("[e3_id={}] Failed to create CRISP contract", e3_id)
                    })?;

                    let receipt = contract
                        .set_merkle_root(e3_id_u256, merkle_root_u256)
                        .await
                        .with_context(|| {
                            format!("[e3_id={}] Failed to call setMerkleRoot", e3_id)
                        })?;

                    info!(
                        "[e3_id={}] setMerkleRoot successful. TxHash: {:?}",
                        e3_id, receipt.transaction_hash
                    );
                }

                Ok(())
            }
        })
        .await;
    Ok(indexer)
}

async fn handle_e3_input_deadline_expiration(
    e3_id: String,
    store: SharedStore<impl DataStore>,
) -> eyre::Result<()> {
    let mut repo = CrispE3Repository::new(store.clone(), &e3_id);
    let e3: e3_sdk::indexer::models::E3 = repo.get_e3().await?;

    repo.update_status("Expired").await?;

    let voter_count = repo.get_vote_count().await?;
    let votes = repo.get_ciphertext_inputs().await?;

    if voter_count > 0 && votes.is_empty() {
        warn!(
            "[e3_id={}] {} voter(s) recorded but no InputPublished ciphertexts indexed — \
             skipping FHE compute (check CRISP indexer + on-chain publishInput)",
            e3_id, voter_count
        );
        repo.update_status("Finished").await?;
        info!("[e3_id={}] E3 request handled successfully.", e3_id);
        return Ok(());
    }

    if !votes.is_empty() {
        info!(
            "[e3_id={}] Starting computation for E3 ({} ciphertext input(s), {} voter(s))",
            e3_id,
            votes.len(),
            voter_count
        );
        repo.update_status("Computing").await?;

        let (id, status) = run_compute(
            &e3_id,
            e3.chain_id,
            e3.interfold_address,
            e3.encryption_scheme_id,
            e3.committee_public_key_hash,
            e3.e3_params,
            votes,
            format!(
                "{}/state/add-result",
                CONFIG.interfold_server_url_for_clients()
            ),
        )
        .await
        .map_err(|e| eyre::eyre!("Error sending run compute request: {e}"))?;

        if id != e3_id {
            return Err(eyre::eyre!(
                "Computation request returned unexpected E3 ID: expected {}, got {}",
                e3_id,
                id
            ));
        }

        if status != "processing" {
            return Err(eyre::eyre!(
                "Computation request failed with status: {}",
                status
            ));
        }

        info!("[e3_id={}] Request Computation for E3", e3_id);

        repo.update_status("PublishingCiphertext").await?;
    } else {
        info!(
            "[e3_id={}] E3 has no votes to decrypt. Setting status to Finished.",
            e3_id
        );
        repo.update_status("Finished").await?;
    }
    info!("[e3_id={}] E3 request handled successfully.", e3_id);

    Ok(())
}

pub async fn register_ciphertext_output_published(
    indexer: InterfoldIndexer<impl DataStore, ReadWrite>,
) -> Result<InterfoldIndexer<impl DataStore, ReadWrite>> {
    // CiphertextOutputPublished
    indexer
        .add_event_handler(move |event: CiphertextOutputPublished, ctx| {
            let store = ctx.store();
            let e3_id = event.e3Id.to_string();
            let mut repo = CrispE3Repository::new(store, &e3_id);
            async move {
                info!("[e3_id={}] Handling CiphertextOutputPublished", e3_id);
                repo.update_status("CiphertextPublished").await?;
                Ok(())
            }
        })
        .await;
    Ok(indexer)
}

pub async fn register_plaintext_output_published(
    indexer: InterfoldIndexer<impl DataStore, ReadWrite>,
) -> Result<InterfoldIndexer<impl DataStore, ReadWrite>> {
    // PlaintextOutputPublished
    indexer
        .add_event_handler(move |event: PlaintextOutputPublished, ctx| {
            let store = ctx.store();
            let e3_id = event.e3Id.to_string();
            let mut repo = CrispE3Repository::new(store, &e3_id);
            async move {
                info!("[e3_id={}] Handling PlaintextOutputPublished", e3_id);

                let num_options = repo.get_num_options().await?;

                // The plaintextOutput from the event contains the result of the FHE computation.
                // Decode the tally using the utility function.
                let vote_counts = decode_tally(&event.plaintextOutput, num_options)?;

                for (i, count) in vote_counts.iter().enumerate() {
                    info!("[e3_id={}] Option index: {} votes: {:?}", e3_id, i, count);
                }

                repo.set_votes(vote_counts).await?;
                repo.update_status("Finished").await?;
                Ok(())
            }
        })
        .await;
    Ok(indexer)
}

pub async fn register_committee_published(
    indexer: InterfoldIndexer<impl DataStore, ReadWrite>,
) -> Result<InterfoldIndexer<impl DataStore, ReadWrite>> {
    // CommitteePublished
    indexer
        .add_event_handler(move |event: CommitteePublished, ctx| {
            async move {
                let store = ctx.store();
                let e3_id = event.e3Id.to_string();
                let mut repo = CrispE3Repository::new(store.clone(), &e3_id);
                let mut current_round_repo = CurrentRoundRepository::new(store);
                info!("[e3_id={}] Handling CommitteePublished", e3_id);
                // Get current time
                let now = get_current_timestamp_rpc().await?;
                info!("[e3_id={}] Current time: {}", event.e3Id, now);

                repo.start_round().await?;

                current_round_repo
                    .set_current_round(CurrentRound { id: e3_id.clone() })
                    .await?;

                let expiration = repo.get_input_deadline().await?;

                info!("[e3_id={}] Registering hook for {}", e3_id, expiration);
                ctx.do_later(expiration, move |_, ctx| {
                    handle_e3_input_deadline_expiration(e3_id.clone(), ctx.store())
                });

                Ok(())
            }
        })
        .await;
    Ok(indexer)
}

pub async fn get_current_timestamp_rpc() -> eyre::Result<u64> {
    let provider = ProviderBuilder::new().connect(&CONFIG.http_rpc_url).await?;
    let block = provider
        .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
        .await?
        .ok_or_else(|| eyre::eyre!("Latest block not found"))?;

    Ok(block.header.timestamp)
}

pub async fn register_input_published(
    indexer: InterfoldIndexer<impl DataStore, ReadWrite>,
) -> Result<InterfoldIndexer<impl DataStore, ReadWrite>> {
    indexer
        .add_event_handler(move |event: InputPublished, ctx| {
            let e3_id = event.e3Id.to_string();
            let store = ctx.store();
            let mut repo = CrispE3Repository::new(store.clone(), &e3_id);
            async move {
                println!(
                    "InputPublished: e3_id={}, index={}, data=0x{}...",
                    event.e3Id,
                    event.index,
                    hex::encode(&event.encryptedVote[..8.min(event.encryptedVote.len())])
                );

                repo.insert_ciphertext_input(event.encryptedVote.to_vec(), event.index.to::<u64>())
                    .await?;
                Ok(())
            }
        })
        .await;
    Ok(indexer)
}

pub async fn start_indexer(
    url: &str,
    contract_address: &str,
    registry_address: &str,
    crisp_address: &str,
    store: SharedStore<impl DataStore>,
    private_key: &str,
) -> Result<()> {
    info!("CRISP: Creating indexer...");
    let crisp_indexer = InterfoldIndexer::new_with_write_contract(
        url,
        &[contract_address, registry_address, crisp_address],
        store,
        private_key,
    )
    .await?;
    info!("CRISP: Indexer registering handlers...");

    let crisp_indexer = register_e3_requested(crisp_indexer).await?;
    let crisp_indexer = register_ciphertext_output_published(crisp_indexer).await?;
    let crisp_indexer = register_plaintext_output_published(crisp_indexer).await?;
    let crisp_indexer = register_committee_published(crisp_indexer).await?;
    let crisp_indexer = register_input_published(crisp_indexer).await?;
    info!("CRISP: Indexer finished registering handlers!");
    crisp_indexer.listen().await?;
    info!("CRISP: Indexer listen loop has finished!");
    Ok(())
}

#[cfg(test)]
mod custom_params_decoding_tests {
    use crate::server::models::CensusMode;
    use alloy::dyn_abi::SolType;
    use alloy::primitives::{Address, U256};
    use alloy::sol_types::sol_data;

    type CustomParamsTuple = (
        sol_data::Address,
        sol_data::Uint<256>,
        sol_data::Uint<256>,
        sol_data::Uint<256>,
        sol_data::Uint<256>,
        sol_data::Uint<256>,
        sol_data::Uint<256>,
    );

    fn encode(census_mode: u64) -> Vec<u8> {
        <CustomParamsTuple as SolType>::abi_encode(&(
            Address::ZERO,
            U256::from(0),
            U256::from(3),
            U256::from(0),
            U256::from(1),
            U256::from(census_mode),
            // Voting-power divisor; 0 means the contract derives it from the token decimals.
            U256::from(0),
        ))
    }

    /// Every producer of `customParams` must encode exactly what `CRISPProgram._initRound`
    /// decodes. A short encoding does not degrade — `abi.decode` reverts with empty data, so
    /// `request_e3` fails with nothing to read, which is how this surfaced in crisp_e2e after the
    /// divisor field was added. Pins the arity so a future field breaks a test instead of a round.
    #[test]
    fn the_encoded_tuple_has_the_arity_the_contract_decodes() {
        // Seven: token, threshold, numOptions, creditMode, credits, censusMode, divisor.
        const CONTRACT_FIELD_COUNT: usize = 7;

        let encoded = encode(0);

        // Each static field occupies one 32-byte word.
        assert_eq!(
            encoded.len(),
            CONTRACT_FIELD_COUNT * 32,
            "encoded params must be {} words; a mismatch reverts request_e3 with empty data",
            CONTRACT_FIELD_COUNT
        );
    }

    #[test]
    fn decodes_the_declared_census_mode() {
        let decoded = <CustomParamsTuple as SolType>::abi_decode(&encode(1)).unwrap();
        assert_eq!(
            CensusMode::try_from(decoded.5.to::<u64>()).unwrap(),
            CensusMode::ByRequester
        );
    }

    /// Params without the field are not a legacy form to tolerate — they are malformed, and must
    /// fail rather than be read as a token vote.
    #[test]
    fn params_missing_the_field_fail_to_decode() {
        type Short = (
            sol_data::Address,
            sol_data::Uint<256>,
            sol_data::Uint<256>,
            sol_data::Uint<256>,
            sol_data::Uint<256>,
        );
        let short = <Short as SolType>::abi_encode(&(
            Address::ZERO,
            U256::from(0),
            U256::from(3),
            U256::from(0),
            U256::from(1),
        ));
        assert!(<CustomParamsTuple as SolType>::abi_decode(&short).is_err());
    }

    #[test]
    fn an_unrecognised_mode_is_an_error() {
        // 3 rather than 2: 2 is `Onchain`. This must stay one past the highest variant, so it
        // keeps testing an unknown mode rather than silently becoming a valid one.
        let decoded = <CustomParamsTuple as SolType>::abi_decode(&encode(3)).unwrap();
        assert!(CensusMode::try_from(decoded.5.to::<u64>()).is_err());
    }
}
