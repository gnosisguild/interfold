// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Full correlated NodeFold E2E with C3a/C3b folded by the isolated Chonk probe.
//!
//! The Rust side creates the correlated C0/C1/C2/C3-leaf/C4 witnesses. The TypeScript side turns
//! the C3 leaves into Chonk rollup proofs and tubes. Those tubes are folded into ordinary C3 proofs
//! and then consumed by the production `c3ab_fold` and `node_fold` circuits.

mod common;
#[path = "common/node_fold_witness.rs"]
mod node_fold_witness;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use alloy::primitives::Address;
use common::{
    find_bb, require_circuits_for_preset_and_committee, require_minimum_circuits_for_preset,
    setup_compiled_circuit_for_preset, setup_recursive_aggregation_fold_circuit_for_preset,
    setup_test_prover,
};
use e3_events::{CircuitName, Proof};
use e3_fhe_params::{build_pair_for_preset, create_deterministic_crp_from_default_seed, BfvPreset};
use e3_polynomial::CrtPolynomial;
use e3_utils::utility_types::ArcBytes;
use e3_zk_helpers::computation::{Computation, DkgInputType};
use e3_zk_helpers::dkg::pk::circuit::{PkCircuit, PkCircuitData};
use e3_zk_helpers::dkg::share_computation::Inputs as ShareComputationInputs;
use e3_zk_helpers::dkg::share_decryption::{ShareDecryptionCircuit, ShareDecryptionCircuitData};
use e3_zk_helpers::dkg::share_encryption::{ShareEncryptionCircuit, ShareEncryptionCircuitData};
use e3_zk_helpers::math::plaintext_poly_u64;
use e3_zk_helpers::threshold::pk_aggregation::{PkAggregationCircuit, PkAggregationCircuitData};
use e3_zk_helpers::threshold::pk_generation::{PkGenerationCircuit, PkGenerationCircuitData};
use e3_zk_helpers::{CiphernodesCommittee, CiphernodesCommitteeSize};
use e3_zk_prover::test_utils::{fold_witness_field_strings, load_vk_artifacts};
use e3_zk_prover::{
    generate_sequential_nodes_fold, prove_dkg_aggregation, prove_node_dkg_fold_with_c3_overrides,
    CircuitVariant, DkgAggregationC3Overrides, DkgAggregationInput, NodeDkgFoldC3Overrides,
    NodeDkgFoldC3Proof, NodeDkgFoldInput, Provable, ZkBackend, ZkProver, DEFAULT_C2_CHUNK_SIZE,
};
use fhe::bfv::{PublicKey, SecretKey};
use fhe::mbfv::{AggregateIter, PublicKeyShare};
use fhe_traits::Serialize as FheSerialize;
use node_fold_witness::{
    pk_generation_sample_with_esi, share_computation_esm_from_esi, share_computation_sk_from_pk,
    share_encryption_for_slot,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct ChonkTubeFixture {
    #[serde(rename = "proofFields")]
    proof_fields: Vec<String>,
    #[serde(rename = "publicInputs")]
    public_inputs: Vec<String>,
    #[serde(rename = "verificationKey")]
    verification_key: Vec<String>,
    #[serde(rename = "keyHash")]
    key_hash: String,
}

#[derive(Debug, Deserialize)]
struct ChonkTubeOutput {
    #[serde(rename = "slotIndices")]
    slot_indices: Vec<usize>,
    c3a: Vec<ChonkTubeFixture>,
    c3b: Vec<ChonkTubeFixture>,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn recursive_aggregation_compiled_json_path(circuit: CircuitName) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../circuits/bin")
        .join(circuit.group())
        .join(circuit.as_str())
        .join("target")
        .join(format!("{}.json", circuit.as_str()))
}

fn c3_fold_json_path() -> PathBuf {
    recursive_aggregation_compiled_json_path(CircuitName::C3Fold)
}

fn c3_fold_total_slots_from_compiled_json() -> usize {
    let path = c3_fold_json_path();
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse c3_fold JSON");
    let length = value["abi"]["parameters"]
        .as_array()
        .and_then(|parameters| {
            parameters.iter().find_map(|parameter| {
                (parameter.get("name")
                    == Some(&serde_json::Value::String("acc_public_inputs".into())))
                .then(|| parameter.get("type")?.get("length")?.as_u64())
                .flatten()
            })
        })
        .expect("c3_fold acc_public_inputs length") as usize;
    (length - 6) / 3
}

fn field_str_zero() -> String {
    format!("0x{}", hex::encode([0u8; 32]))
}

fn proof_public_fields(proof: &Proof) -> Vec<String> {
    fold_witness_field_strings(proof.public_signals.as_ref()).expect("public signals as fields")
}

fn leaf_fixture(proof: &Proof) -> serde_json::Value {
    json!({
        "proof": fold_witness_field_strings(proof.data.as_ref()).expect("proof as fields"),
        "publicInputs": proof_public_fields(proof),
    })
}

fn write_chonk_leaf_fixtures(
    path: &std::path::Path,
    c3a: &[Proof],
    c3b: &[Proof],
    slots: &[usize],
) {
    let input = json!({
        "slotIndices": slots,
        "c3a": c3a.iter().map(leaf_fixture).collect::<Vec<_>>(),
        "c3b": c3b.iter().map(leaf_fixture).collect::<Vec<_>>(),
    });
    fs::write(
        path,
        serde_json::to_vec_pretty(&input).expect("serialize Chonk input"),
    )
    .expect("write Chonk input");
}

fn fields_to_bytes(fields: &[String]) -> Vec<u8> {
    fields
        .iter()
        .flat_map(|field| {
            let value = field.strip_prefix("0x").unwrap_or(field);
            let padded = format!("{value:0>64}");
            let bytes = hex::decode(padded).expect("field hex");
            assert_eq!(bytes.len(), 32, "field must be 32 bytes");
            bytes
        })
        .collect()
}

fn tube_proof(fixture: &ChonkTubeFixture, total_slots: usize) -> Proof {
    assert_eq!(
        fixture.public_inputs.len(),
        6 + (3 * total_slots),
        "C3 tube public-input width"
    );
    assert_eq!(
        fixture.proof_fields.len(),
        480,
        "RollupHonk proof field count"
    );
    Proof::new(
        CircuitName::C3Fold,
        ArcBytes::from_bytes(&fields_to_bytes(&fixture.proof_fields)),
        ArcBytes::from_bytes(&fields_to_bytes(&fixture.public_inputs)),
    )
}

fn tube_proofs(fixtures: &[ChonkTubeFixture], total_slots: usize) -> Vec<Proof> {
    assert_eq!(fixtures.len(), 2, "Chonk C3 chunk count");
    let first = fixtures.first().expect("Chonk C3 tube fixture");
    for fixture in fixtures {
        assert_eq!(
            fixture.verification_key, first.verification_key,
            "Chonk tube VK mismatch"
        );
        assert_eq!(
            fixture.key_hash, first.key_hash,
            "Chonk tube VK hash mismatch"
        );
    }
    fixtures
        .iter()
        .map(|fixture| tube_proof(fixture, total_slots))
        .collect()
}

struct ChonkNodeMaterial {
    party_id: usize,
    pk_generation: PkGenerationCircuitData,
    c0_proof: Proof,
    c1_proof: Proof,
    c2a_proof: Proof,
    c2b_proof: Proof,
    c3a_inner_proofs: Vec<Proof>,
    c3b_inner_proofs: Vec<Proof>,
    c3_slot_indices: Vec<u32>,
    c3a_data: Vec<ShareEncryptionCircuitData>,
    c3b_data: Vec<ShareEncryptionCircuitData>,
    c4a_proof: Option<Proof>,
    c4b_proof: Option<Proof>,
}

struct ChonkNodeTubes {
    slot_indices: Vec<usize>,
    c3a: Vec<Proof>,
    c3b: Vec<Proof>,
    c3a_verification_key: Vec<String>,
    c3b_verification_key: Vec<String>,
    c3a_key_hash: String,
    c3b_key_hash: String,
}

fn stage_dkg_aggregator_evm(prover: &ZkProver, artifacts_dir: &str) {
    let source = repo_root().join("circuits/bin/recursive_aggregation/dkg_aggregator/target");
    let json = source.join("dkg_aggregator.json");
    let evm_vk = source.join("dkg_aggregator.vk");
    let evm_hash = source.join("dkg_aggregator.vk_hash");
    assert!(json.exists(), "missing {}", json.display());
    assert!(evm_vk.exists(), "missing {}", evm_vk.display());
    assert!(evm_hash.exists(), "missing {}", evm_hash.display());

    let default_dir = prover
        .circuits_dir(CircuitVariant::Default, artifacts_dir)
        .join(CircuitName::DkgAggregator.dir_path());
    let evm_dir = prover
        .circuits_dir(CircuitVariant::Evm, artifacts_dir)
        .join(CircuitName::DkgAggregator.dir_path());
    fs::create_dir_all(&default_dir).expect("create DkgAggregator default directory");
    fs::create_dir_all(&evm_dir).expect("create DkgAggregator EVM directory");
    fs::copy(&json, default_dir.join("dkg_aggregator.json")).expect("stage DkgAggregator JSON");
    fs::copy(&json, evm_dir.join("dkg_aggregator.json")).expect("stage DkgAggregator EVM JSON");
    fs::copy(&evm_vk, evm_dir.join("dkg_aggregator.vk")).expect("stage DkgAggregator EVM VK");
    fs::copy(&evm_hash, evm_dir.join("dkg_aggregator.vk_hash"))
        .expect("stage DkgAggregator EVM VK hash");
}

async fn setup_chonk_multi_node_test(
    committee_size: CiphernodesCommitteeSize,
) -> Option<(
    ZkBackend,
    tempfile::TempDir,
    ZkProver,
    BfvPreset,
    String,
    CiphernodesCommittee,
)> {
    let preset = BfvPreset::InsecureThreshold512;
    let Some(bb) = find_bb().await else {
        println!("skipping: bb not found");
        return None;
    };
    if require_circuits_for_preset_and_committee(preset, committee_size.as_str()).is_none() {
        return None;
    }
    if !c3_fold_json_path().exists()
        || !recursive_aggregation_compiled_json_path(CircuitName::NodeFold).exists()
        || !recursive_aggregation_compiled_json_path(CircuitName::C3ChunkFold).exists()
        || !recursive_aggregation_compiled_json_path(CircuitName::NodesFold).exists()
        || !recursive_aggregation_compiled_json_path(CircuitName::NodesFoldKernel).exists()
        || !recursive_aggregation_compiled_json_path(CircuitName::DkgAggregator).exists()
    {
        println!("skipping: production recursive aggregation artifacts are not built");
        return None;
    }

    let committee = committee_size.values();
    let (backend, temp) = setup_test_prover(&bb).await;
    let prover = ZkProver::new(&backend);
    let committee_name = committee_size.as_str();
    let artifacts_dir = preset.artifacts_dir_for_committee(committee_name);

    for group_circuit in [
        ("dkg", "pk"),
        ("dkg", "sk_share_computation_chunk"),
        ("dkg", "esm_share_computation_chunk"),
        ("dkg", "share_encryption"),
        ("dkg", "share_decryption"),
        ("threshold", "pk_generation"),
        ("threshold", "pk_aggregation"),
    ] {
        setup_compiled_circuit_for_preset(
            &backend,
            group_circuit.0,
            group_circuit.1,
            preset,
            committee_name,
        )
        .await;
    }
    for circuit in [
        CircuitName::C2ChunkBatch,
        CircuitName::SkC2ChunkFinalize,
        CircuitName::ESmC2ChunkFinalize,
        CircuitName::C2abChunkFold,
        CircuitName::C3Fold,
        CircuitName::C3FoldKernel,
        CircuitName::C3ChunkFold,
        CircuitName::C3abFold,
        CircuitName::C3abFoldSequential,
        CircuitName::C4abFold,
        CircuitName::NodeFold,
        CircuitName::NodesFold,
        CircuitName::NodesFoldKernel,
    ] {
        setup_recursive_aggregation_fold_circuit_for_preset(
            &backend,
            circuit,
            preset,
            committee_name,
        )
        .await;
    }
    stage_dkg_aggregator_evm(&prover, &artifacts_dir);

    Some((backend, temp, prover, preset, artifacts_dir, committee))
}

fn threshold_public_key_share(
    preset: BfvPreset,
    pk_generation: &PkGenerationCircuitData,
) -> PublicKeyShare {
    let (threshold_params, _) = build_pair_for_preset(preset).expect("threshold parameters");
    let context = threshold_params
        .context_at_level(0)
        .expect("threshold level-0 context");
    let p0 = pk_generation
        .pk0_share
        .to_fhe_polynomial(&context, threshold_params.moduli())
        .expect("convert C1 pk0 share to FHE polynomial")
        .into_ntt();
    let crp = create_deterministic_crp_from_default_seed(&threshold_params);
    PublicKeyShare::deserialize(&p0.to_bytes(), &threshold_params, crp)
        .expect("deserialize C1 public key share")
}

fn build_chonk_node_material(
    prover: &ZkProver,
    preset: BfvPreset,
    committee: CiphernodesCommittee,
    artifacts_dir: &str,
    party_id: usize,
    dkg_sks: &[SecretKey],
    dkg_pks: &[PublicKey],
    total_slots: usize,
) -> ChonkNodeMaterial {
    let (pk_generation, esi, pk_secret_key) =
        pk_generation_sample_with_esi(preset, committee.clone()).expect("correlated pk sample");
    let share_sk =
        share_computation_sk_from_pk(preset, committee.clone(), &pk_generation, &pk_secret_key)
            .expect("correlated C2a data");
    let share_esm = share_computation_esm_from_esi(preset, committee.clone(), &pk_generation, &esi)
        .expect("correlated C2b data");
    let sk_inputs = ShareComputationInputs::compute(preset, &share_sk).expect("C2a inputs");
    let esm_inputs = ShareComputationInputs::compute(preset, &share_esm).expect("C2b inputs");

    let c0_data = PkCircuitData {
        public_key: dkg_pks[party_id].clone(),
    };
    let c0_proof = PkCircuit
        .prove_with_variant(
            prover,
            &preset,
            &c0_data,
            &format!("e3-chonk-multi-c0-{party_id}"),
            CircuitVariant::Recursive,
            artifacts_dir,
        )
        .expect("C0 proof");
    let c1_proof = PkGenerationCircuit
        .prove_with_variant(
            prover,
            &preset,
            &pk_generation,
            &format!("e3-chonk-multi-c1-{party_id}"),
            CircuitVariant::Recursive,
            artifacts_dir,
        )
        .expect("C1 proof");
    let c2a_proof = e3_zk_prover::prove_chunked_share_computation(
        prover,
        preset,
        &share_sk,
        &format!("e3-chonk-multi-c2a-{party_id}"),
        artifacts_dir,
    )
    .expect("C2a proof")
    .proof;
    let c2b_proof = e3_zk_prover::prove_chunked_share_computation(
        prover,
        preset,
        &share_esm,
        &format!("e3-chonk-multi-c2b-{party_id}"),
        artifacts_dir,
    )
    .expect("C2b proof")
    .proof;

    let l = preset.metadata().num_moduli;
    let slots_per_party = total_slots / committee.n;
    let c3_slot_indices: Vec<usize> = (0..total_slots)
        .filter(|slot| slot / slots_per_party != party_id)
        .collect();
    assert_eq!(c3_slot_indices.len(), total_slots - slots_per_party);
    let mut c3a_data = Vec::with_capacity(total_slots);
    let mut c3b_data = Vec::with_capacity(total_slots);
    for slot in 0..total_slots {
        let recipient = slot / l;
        c3a_data.push(
            share_encryption_for_slot(
                preset,
                &dkg_sks[recipient],
                &dkg_pks[recipient],
                &sk_inputs,
                slot,
                DkgInputType::SecretKey,
                committee.clone(),
            )
            .expect("C3a slot data"),
        );
        c3b_data.push(
            share_encryption_for_slot(
                preset,
                &dkg_sks[recipient],
                &dkg_pks[recipient],
                &esm_inputs,
                slot,
                DkgInputType::SmudgingNoise,
                committee.clone(),
            )
            .expect("C3b slot data"),
        );
    }

    let c3a_inner_proofs = c3_slot_indices
        .iter()
        .map(|&slot| {
            ShareEncryptionCircuit
                .prove_with_variant(
                    prover,
                    &preset,
                    &c3a_data[slot],
                    &format!("e3-chonk-multi-c3a-{party_id}-{slot}"),
                    CircuitVariant::Recursive,
                    artifacts_dir,
                )
                .expect("C3a proof")
        })
        .collect();
    let c3b_inner_proofs = c3_slot_indices
        .iter()
        .map(|&slot| {
            ShareEncryptionCircuit
                .prove_with_variant(
                    prover,
                    &preset,
                    &c3b_data[slot],
                    &format!("e3-chonk-multi-c3b-{party_id}-{slot}"),
                    CircuitVariant::Recursive,
                    artifacts_dir,
                )
                .expect("C3b proof")
        })
        .collect();

    ChonkNodeMaterial {
        party_id,
        pk_generation,
        c0_proof,
        c1_proof,
        c2a_proof,
        c2b_proof,
        c3a_inner_proofs,
        c3b_inner_proofs,
        c3_slot_indices: c3_slot_indices.iter().map(|&slot| slot as u32).collect(),
        c3a_data,
        c3b_data,
        c4a_proof: None,
        c4b_proof: None,
    }
}

fn populate_chonk_c4_proofs(
    prover: &ZkProver,
    preset: BfvPreset,
    committee: CiphernodesCommittee,
    artifacts_dir: &str,
    materials: &mut [ChonkNodeMaterial],
    dkg_sks: &[SecretKey],
) {
    let l = preset.metadata().num_moduli;
    for recipient in 0..committee.h {
        let mut c4a_ciphertexts = Vec::with_capacity(committee.h);
        let mut c4b_ciphertexts = Vec::with_capacity(committee.h);
        let mut own_a = Vec::with_capacity(l);
        let mut own_b = Vec::with_capacity(l);
        for sender in 0..committee.h {
            if sender == recipient {
                own_a = (0..l)
                    .map(|modulus| {
                        plaintext_poly_u64(
                            &materials[sender].c3a_data[recipient * l + modulus].plaintext,
                        )
                        .expect("C4a own plaintext")
                    })
                    .collect();
                own_b = (0..l)
                    .map(|modulus| {
                        plaintext_poly_u64(
                            &materials[sender].c3b_data[recipient * l + modulus].plaintext,
                        )
                        .expect("C4b own plaintext")
                    })
                    .collect();
                c4a_ciphertexts.push(None);
                c4b_ciphertexts.push(None);
            } else {
                c4a_ciphertexts.push(Some(
                    (0..l)
                        .map(|modulus| {
                            materials[sender].c3a_data[recipient * l + modulus]
                                .ciphertext
                                .clone()
                        })
                        .collect(),
                ));
                c4b_ciphertexts.push(Some(
                    (0..l)
                        .map(|modulus| {
                            materials[sender].c3b_data[recipient * l + modulus]
                                .ciphertext
                                .clone()
                        })
                        .collect(),
                ));
            }
        }

        let c4a_data = ShareDecryptionCircuitData {
            secret_key: dkg_sks[recipient].clone(),
            honest_ciphertexts: c4a_ciphertexts,
            recipient_party_id: recipient as u64,
            own_plaintext_share: own_a,
            dkg_input_type: DkgInputType::SecretKey,
            chunk_size: DEFAULT_C2_CHUNK_SIZE as u32,
            committee: committee.clone(),
        };
        let c4b_data = ShareDecryptionCircuitData {
            secret_key: dkg_sks[recipient].clone(),
            honest_ciphertexts: c4b_ciphertexts,
            recipient_party_id: recipient as u64,
            own_plaintext_share: own_b,
            dkg_input_type: DkgInputType::SmudgingNoise,
            chunk_size: DEFAULT_C2_CHUNK_SIZE as u32,
            committee: committee.clone(),
        };
        materials[recipient].c4a_proof = Some(
            ShareDecryptionCircuit
                .prove_with_variant(
                    prover,
                    &preset,
                    &c4a_data,
                    &format!("e3-chonk-multi-c4a-{recipient}"),
                    CircuitVariant::Recursive,
                    artifacts_dir,
                )
                .expect("C4a proof"),
        );
        materials[recipient].c4b_proof = Some(
            ShareDecryptionCircuit
                .prove_with_variant(
                    prover,
                    &preset,
                    &c4b_data,
                    &format!("e3-chonk-multi-c4b-{recipient}"),
                    CircuitVariant::Recursive,
                    artifacts_dir,
                )
                .expect("C4b proof"),
        );
    }
}

fn run_chonk_probe_for_node(
    temp: &tempfile::TempDir,
    node: &ChonkNodeMaterial,
    committee: CiphernodesCommitteeSize,
    total_slots: usize,
) -> ChonkNodeTubes {
    let leaf_input_path = temp
        .path()
        .join(format!("chonk-c3-multi-leaves-{}.json", node.party_id));
    let tube_output_path = temp
        .path()
        .join(format!("chonk-c3-multi-tubes-{}.json", node.party_id));
    let slot_indices: Vec<usize> = node
        .c3_slot_indices
        .iter()
        .map(|&slot| slot as usize)
        .collect();
    write_chonk_leaf_fixtures(
        &leaf_input_path,
        &node.c3a_inner_proofs,
        &node.c3b_inner_proofs,
        &slot_indices,
    );
    let probe_status = Command::new("pnpm")
        .current_dir(repo_root())
        .args(["--dir", "packages/interfold-sdk", "probe:chonk-c3"])
        .env("CHONK_C3_LEAF_FIXTURES", &leaf_input_path)
        .env("CHONK_C3_OUTPUT", &tube_output_path)
        .env("CHONK_C3_COMMITTEE", committee.as_str())
        .status()
        .expect("run Chonk C3 probe");
    assert!(probe_status.success(), "Chonk C3 probe failed");
    let output: ChonkTubeOutput =
        serde_json::from_slice(&fs::read(&tube_output_path).expect("read Chonk tube output"))
            .expect("parse Chonk tube output");
    let ChonkTubeOutput {
        slot_indices: output_slot_indices,
        c3a,
        c3b,
    } = output;
    assert_eq!(output_slot_indices, slot_indices);
    let c3a_tubes = tube_proofs(&c3a, total_slots);
    let c3b_tubes = tube_proofs(&c3b, total_slots);
    let c3a_fixture = c3a.first().expect("C3a tube fixture");
    let c3b_fixture = c3b.first().expect("C3b tube fixture");
    ChonkNodeTubes {
        slot_indices: output_slot_indices,
        c3a: c3a_tubes,
        c3b: c3b_tubes,
        c3a_verification_key: c3a_fixture.verification_key.clone(),
        c3b_verification_key: c3b_fixture.verification_key.clone(),
        c3a_key_hash: c3a_fixture.key_hash.clone(),
        c3b_key_hash: c3b_fixture.key_hash.clone(),
    }
}

fn assert_chonk_c3_bindings(
    c3a: &[Proof],
    c3b: &[Proof],
    c2a: &Proof,
    c2b: &Proof,
    leaf_key_hash: &str,
    kernel_key_hash: &str,
    replacement_key_hash: &str,
    party_id: usize,
    slots_per_party: usize,
    total_slots: usize,
    slot_indices: &[usize],
) {
    let c2a_public = proof_public_fields(c2a);
    let c2b_public = proof_public_fields(c2b);
    assert_eq!(c3a.len(), 2, "C3a Chonk chunk count");
    assert_eq!(c3b.len(), 2, "C3b Chonk chunk count");
    assert_eq!(c3a.len(), c3b.len());
    let non_local_slots = total_slots - slots_per_party;
    assert_eq!(
        slot_indices.len(),
        non_local_slots,
        "C3 non-local slot count"
    );
    assert_eq!(non_local_slots % c3a.len(), 0, "C3 slots must split evenly");
    let chunk_size = non_local_slots / c3a.len();
    let prefix_len = 6;
    for (chunk, (c3a_proof, c3b_proof)) in c3a.iter().zip(c3b).enumerate() {
        let c3a_public = proof_public_fields(c3a_proof);
        let c3b_public = proof_public_fields(c3b_proof);
        assert_eq!(c3a_public.len(), prefix_len + (3 * total_slots));
        assert_eq!(c3b_public.len(), prefix_len + (3 * total_slots));
        assert_eq!(&c3a_public[0], leaf_key_hash);
        assert_eq!(&c3b_public[0], leaf_key_hash);
        assert_eq!(&c3a_public[1], replacement_key_hash);
        assert_eq!(&c3b_public[1], replacement_key_hash);
        assert_eq!(&c3a_public[4], kernel_key_hash);
        assert_eq!(&c3b_public[4], kernel_key_hash);
        assert_eq!(&c3a_public[5], replacement_key_hash);
        assert_eq!(&c3b_public[5], replacement_key_hash);

        let chunk_slots = &slot_indices[chunk * chunk_size..(chunk + 1) * chunk_size];
        for slot in 0..total_slots {
            if slot / slots_per_party == party_id || !chunk_slots.contains(&slot) {
                for column in 0..3 {
                    assert_eq!(
                        c3a_public[prefix_len + (column * total_slots) + slot],
                        field_str_zero(),
                        "C3a chunk {chunk} inactive slot {slot}, column {column}"
                    );
                    assert_eq!(
                        c3b_public[prefix_len + (column * total_slots) + slot],
                        field_str_zero(),
                        "C3b chunk {chunk} inactive slot {slot}, column {column}"
                    );
                }
            } else {
                assert_eq!(
                    c3a_public[prefix_len + total_slots + slot],
                    c2a_public[2 + slot],
                    "C3a message/C2a share binding at slot {slot}"
                );
                assert_eq!(
                    c3b_public[prefix_len + total_slots + slot],
                    c2b_public[2 + slot],
                    "C3b message/C2b share binding at slot {slot}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "benchmark: real Chonk C3a/C3b through c3ab_fold and node_fold"]
async fn chonk_c3_flows_through_correlated_node_fold() {
    let preset = BfvPreset::InsecureThreshold512;
    let Some(bb) = find_bb().await else {
        println!("skipping: bb not found");
        return;
    };
    if require_minimum_circuits_for_preset(preset).is_none() {
        return;
    }
    if !c3_fold_json_path().exists()
        || !recursive_aggregation_compiled_json_path(CircuitName::NodeFold).exists()
    {
        println!("skipping: production recursive aggregation artifacts are not built");
        return;
    }

    let committee = CiphernodesCommitteeSize::Minimum.values();
    let (backend, temp) = setup_test_prover(&bb).await;
    let prover = ZkProver::new(&backend);
    let artifacts_dir = preset.artifacts_dir_for_committee("minimum");

    for group_circuit in [
        ("dkg", "pk"),
        ("dkg", "sk_share_computation_chunk"),
        ("dkg", "esm_share_computation_chunk"),
        ("dkg", "share_encryption"),
        ("dkg", "share_decryption"),
        ("threshold", "pk_generation"),
    ] {
        setup_compiled_circuit_for_preset(
            &backend,
            group_circuit.0,
            group_circuit.1,
            preset,
            "minimum",
        )
        .await;
    }
    for circuit in [
        CircuitName::C2ChunkBatch,
        CircuitName::SkC2ChunkFinalize,
        CircuitName::ESmC2ChunkFinalize,
        CircuitName::C2abChunkFold,
        CircuitName::C3Fold,
        CircuitName::C3FoldKernel,
        CircuitName::C3ChunkFold,
        CircuitName::C3abFold,
        CircuitName::C4abFold,
        CircuitName::NodeFold,
    ] {
        setup_recursive_aggregation_fold_circuit_for_preset(&backend, circuit, preset, "minimum")
            .await;
    }

    let (pk_gen, esi, pk_secret_key) =
        pk_generation_sample_with_esi(preset, committee.clone()).expect("correlated pk sample");
    let share_sk = share_computation_sk_from_pk(preset, committee.clone(), &pk_gen, &pk_secret_key)
        .expect("correlated C2a data");
    let share_esm = share_computation_esm_from_esi(preset, committee.clone(), &pk_gen, &esi)
        .expect("correlated C2b data");
    let sk_inputs = ShareComputationInputs::compute(preset, &share_sk).expect("C2a inputs");
    let esm_inputs = ShareComputationInputs::compute(preset, &share_esm).expect("C2b inputs");

    let pk_bfv_data = PkCircuitData::generate_sample(preset).expect("C0 sample");
    let c0_proof = PkCircuit
        .prove_with_variant(
            &prover,
            &preset,
            &pk_bfv_data,
            "e3-chonk-nf-c0",
            CircuitVariant::Recursive,
            &artifacts_dir,
        )
        .expect("C0 proof");
    let c1_proof = PkGenerationCircuit
        .prove_with_variant(
            &prover,
            &preset,
            &pk_gen,
            "e3-chonk-nf-c1",
            CircuitVariant::Recursive,
            &artifacts_dir,
        )
        .expect("C1 proof");

    let (_, dkg_params) = e3_fhe_params::build_pair_for_preset(preset).expect("DKG params");
    let mut rng = rand::rng();
    let dkg_sk = fhe::bfv::SecretKey::random(&dkg_params, &mut rng);
    let dkg_pk = fhe::bfv::PublicKey::new(&dkg_sk, &mut rng);

    let total_slots = c3_fold_total_slots_from_compiled_json();
    let expected_slots = committee.n * preset.metadata().num_moduli;
    assert_eq!(total_slots, expected_slots);
    let slots_per_party = total_slots / committee.n;
    let own_party_id = 0usize;
    let slot_indices: Vec<usize> = (slots_per_party..total_slots).collect();
    let slot_indices_u32: Vec<u32> = slot_indices.iter().map(|&slot| slot as u32).collect();

    let mut c3a_inners = Vec::with_capacity(slot_indices.len());
    let mut c3b_inners = Vec::with_capacity(slot_indices.len());
    for &slot in &slot_indices {
        let c3a_data = share_encryption_for_slot(
            preset,
            &dkg_sk,
            &dkg_pk,
            &sk_inputs,
            slot,
            DkgInputType::SecretKey,
            committee.clone(),
        )
        .expect("C3a slot data");
        let c3b_data = share_encryption_for_slot(
            preset,
            &dkg_sk,
            &dkg_pk,
            &esm_inputs,
            slot,
            DkgInputType::SmudgingNoise,
            committee.clone(),
        )
        .expect("C3b slot data");
        c3a_inners.push(
            ShareEncryptionCircuit
                .prove_with_variant(
                    &prover,
                    &preset,
                    &c3a_data,
                    &format!("e3-chonk-nf-c3a-{slot}"),
                    CircuitVariant::Recursive,
                    &artifacts_dir,
                )
                .expect("C3a proof"),
        );
        c3b_inners.push(
            ShareEncryptionCircuit
                .prove_with_variant(
                    &prover,
                    &preset,
                    &c3b_data,
                    &format!("e3-chonk-nf-c3b-{slot}"),
                    CircuitVariant::Recursive,
                    &artifacts_dir,
                )
                .expect("C3b proof"),
        );
    }

    let c4a_data = ShareDecryptionCircuitData::generate_sample(
        preset,
        committee.clone(),
        DkgInputType::SecretKey,
    )
    .expect("C4a sample");
    let c4b_data = ShareDecryptionCircuitData::generate_sample(
        preset,
        committee.clone(),
        DkgInputType::SmudgingNoise,
    )
    .expect("C4b sample");
    let c4a_proof = ShareDecryptionCircuit
        .prove_with_variant(
            &prover,
            &preset,
            &c4a_data,
            "e3-chonk-nf-c4a",
            CircuitVariant::Recursive,
            &artifacts_dir,
        )
        .expect("C4a proof");
    let c4b_proof = ShareDecryptionCircuit
        .prove_with_variant(
            &prover,
            &preset,
            &c4b_data,
            "e3-chonk-nf-c4b",
            CircuitVariant::Recursive,
            &artifacts_dir,
        )
        .expect("C4b proof");

    let c2a_chunked = e3_zk_prover::prove_chunked_share_computation(
        &prover,
        preset,
        &share_sk,
        "e3-chonk-nf-c2a",
        &artifacts_dir,
    )
    .expect("C2a proof");
    let c2b_chunked = e3_zk_prover::prove_chunked_share_computation(
        &prover,
        preset,
        &share_esm,
        "e3-chonk-nf-c2b",
        &artifacts_dir,
    )
    .expect("C2b proof");

    let leaf_input_path = temp.path().join("chonk-c3-leaves.json");
    let tube_output_path = temp.path().join("chonk-c3-tubes.json");
    write_chonk_leaf_fixtures(&leaf_input_path, &c3a_inners, &c3b_inners, &slot_indices);
    let probe_status = Command::new("pnpm")
        .current_dir(repo_root())
        .args(["--dir", "packages/interfold-sdk", "probe:chonk-c3"])
        .env("CHONK_C3_LEAF_FIXTURES", &leaf_input_path)
        .env("CHONK_C3_OUTPUT", &tube_output_path)
        .env("CHONK_C3_COMMITTEE", "minimum")
        .status()
        .expect("run Chonk C3 probe");
    assert!(probe_status.success(), "Chonk C3 probe failed");

    let chonk_output: ChonkTubeOutput =
        serde_json::from_slice(&fs::read(&tube_output_path).expect("read Chonk tube output"))
            .expect("parse Chonk tube output");
    assert_eq!(chonk_output.slot_indices, slot_indices);
    let c3a_tubes = tube_proofs(&chonk_output.c3a, total_slots);
    let c3b_tubes = tube_proofs(&chonk_output.c3b, total_slots);
    let c3a_fixture = chonk_output.c3a.first().expect("C3a tube fixture");
    let c3b_fixture = chonk_output.c3b.first().expect("C3b tube fixture");

    let leaf_vk = load_vk_artifacts(
        &prover.circuits_dir(CircuitVariant::Recursive, &artifacts_dir),
        CircuitName::ShareEncryption,
    )
    .expect("ShareEncryption VK");
    let kernel_vk = load_vk_artifacts(
        &prover.circuits_dir(CircuitVariant::Default, &artifacts_dir),
        CircuitName::C3FoldKernel,
    )
    .expect("C3 fold kernel VK");
    assert_chonk_c3_bindings(
        &c3a_tubes,
        &c3b_tubes,
        &c2a_chunked.proof,
        &c2b_chunked.proof,
        &leaf_vk.key_hash,
        &kernel_vk.key_hash,
        &c3a_fixture.key_hash,
        own_party_id,
        slots_per_party,
        total_slots,
        &chonk_output.slot_indices,
    );
    assert_eq!(c3a_fixture.key_hash, c3b_fixture.key_hash);

    let overrides = NodeDkgFoldC3Overrides {
        c3a: NodeDkgFoldC3Proof {
            proofs: &c3a_tubes,
            verification_key: &c3a_fixture.verification_key,
            key_hash: &c3a_fixture.key_hash,
        },
        c3b: NodeDkgFoldC3Proof {
            proofs: &c3b_tubes,
            verification_key: &c3b_fixture.verification_key,
            key_hash: &c3b_fixture.key_hash,
        },
    };
    let node = prove_node_dkg_fold_with_c3_overrides(
        &prover,
        &NodeDkgFoldInput {
            c0_proof: &c0_proof,
            c1_proof: &c1_proof,
            c2a_proof: &c2a_chunked.proof,
            c2b_proof: &c2b_chunked.proof,
            c3a_inner_proofs: &c3a_inners,
            c3b_inner_proofs: &c3b_inners,
            c3_slot_indices_a: &slot_indices_u32,
            c3_slot_indices_b: &slot_indices_u32,
            c3_total_slots: total_slots,
            c4a_proof: &c4a_proof,
            c4b_proof: &c4b_proof,
            party_id: own_party_id as u64,
        },
        "e3-chonk-node-fold",
        &artifacts_dir,
        Some(&overrides),
    )
    .expect("NodeFold with Chonk C3 folds");

    assert!(
        prover
            .verify_fold_proof(&node.proof, "e3-chonk-node-fold-verify", 0, &artifacts_dir)
            .expect("NodeFold verification invocation"),
        "NodeFold proof should verify"
    );
    assert!(node
        .step_timings
        .iter()
        .any(|step| step.step == "c3ab_fold"));
    assert!(node
        .step_timings
        .iter()
        .any(|step| step.step == "node_fold"));

    drop(temp);
}

async fn run_multi_node_dkg_aggregator(committee_size: CiphernodesCommitteeSize, use_chonk: bool) {
    let Some((_backend, temp, prover, preset, artifacts_dir, committee)) =
        setup_chonk_multi_node_test(committee_size).await
    else {
        return;
    };

    let total_slots = c3_fold_total_slots_from_compiled_json();
    let expected_slots = committee.n * preset.metadata().num_moduli;
    assert_eq!(total_slots, expected_slots);

    let (_, dkg_params) = build_pair_for_preset(preset).expect("DKG parameters");
    let mut rng = rand::rng();
    let dkg_sks: Vec<SecretKey> = (0..committee.n)
        .map(|_| SecretKey::random(&dkg_params, &mut rng))
        .collect();
    let dkg_pks: Vec<PublicKey> = dkg_sks
        .iter()
        .map(|secret_key| PublicKey::new(secret_key, &mut rng))
        .collect();

    let mut materials = (0..committee.h)
        .map(|party_id| {
            println!(
                "Building classic/Chonk material for node {party_id}/{}",
                committee.h
            );
            build_chonk_node_material(
                &prover,
                preset,
                committee.clone(),
                &artifacts_dir,
                party_id,
                &dkg_sks,
                &dkg_pks,
                total_slots,
            )
        })
        .collect::<Vec<_>>();
    println!("Node material complete for {} nodes", materials.len());

    let tubes = if use_chonk {
        let tubes = materials
            .iter()
            .map(|material| run_chonk_probe_for_node(&temp, material, committee_size, total_slots))
            .collect::<Vec<_>>();
        println!("Chonk probes complete for {} nodes", tubes.len());
        Some(tubes)
    } else {
        None
    };
    if let Some(tubes) = &tubes {
        let leaf_vk = load_vk_artifacts(
            &prover.circuits_dir(CircuitVariant::Recursive, &artifacts_dir),
            CircuitName::ShareEncryption,
        )
        .expect("ShareEncryption VK");
        let kernel_vk = load_vk_artifacts(
            &prover.circuits_dir(CircuitVariant::Default, &artifacts_dir),
            CircuitName::C3FoldKernel,
        )
        .expect("C3 fold kernel VK");
        let slots_per_party = total_slots / committee.n;
        for (material, tube) in materials.iter().zip(tubes) {
            assert_chonk_c3_bindings(
                &tube.c3a,
                &tube.c3b,
                &material.c2a_proof,
                &material.c2b_proof,
                &leaf_vk.key_hash,
                &kernel_vk.key_hash,
                &tube.c3a_key_hash,
                material.party_id,
                slots_per_party,
                total_slots,
                &tube.slot_indices,
            );
            assert_eq!(tube.c3a_key_hash, tube.c3b_key_hash);
            assert_eq!(tube.c3a_verification_key, tube.c3b_verification_key);
        }
        for tube in tubes.iter().skip(1) {
            assert_eq!(tube.c3a_key_hash, tubes[0].c3a_key_hash);
            assert_eq!(tube.c3a_verification_key, tubes[0].c3a_verification_key);
        }
    }

    populate_chonk_c4_proofs(
        &prover,
        preset,
        committee.clone(),
        &artifacts_dir,
        &mut materials,
        &dkg_sks,
    );
    println!("C4 proofs complete for {} nodes", materials.len());

    let mut node_fold_proofs = Vec::with_capacity(committee.h);
    for (material_index, material) in materials.iter().enumerate() {
        println!("Proving NodeFold for node {}", material.party_id);
        let c4a_proof = material.c4a_proof.as_ref().expect("C4a proof");
        let c4b_proof = material.c4b_proof.as_ref().expect("C4b proof");
        let input = NodeDkgFoldInput {
            c0_proof: &material.c0_proof,
            c1_proof: &material.c1_proof,
            c2a_proof: &material.c2a_proof,
            c2b_proof: &material.c2b_proof,
            c3a_inner_proofs: &material.c3a_inner_proofs,
            c3b_inner_proofs: &material.c3b_inner_proofs,
            c3_slot_indices_a: &material.c3_slot_indices,
            c3_slot_indices_b: &material.c3_slot_indices,
            c3_total_slots: total_slots,
            c4a_proof,
            c4b_proof,
            party_id: material.party_id as u64,
        };
        let node = if let Some(tubes) = &tubes {
            let tube = &tubes[material_index];
            let overrides = NodeDkgFoldC3Overrides {
                c3a: NodeDkgFoldC3Proof {
                    proofs: &tube.c3a,
                    verification_key: &tube.c3a_verification_key,
                    key_hash: &tube.c3a_key_hash,
                },
                c3b: NodeDkgFoldC3Proof {
                    proofs: &tube.c3b,
                    verification_key: &tube.c3b_verification_key,
                    key_hash: &tube.c3b_key_hash,
                },
            };
            prove_node_dkg_fold_with_c3_overrides(
                &prover,
                &input,
                &format!("e3-chonk-multi-node-fold-{}", material.party_id),
                &artifacts_dir,
                Some(&overrides),
            )
            .expect("NodeFold with Chonk C3 folds")
        } else {
            e3_zk_prover::prove_node_dkg_fold(
                &prover,
                &input,
                &format!("e3-classic-multi-node-fold-{}", material.party_id),
                &artifacts_dir,
            )
            .expect("classic NodeFold")
        };
        assert!(prover
            .verify_fold_proof(
                &node.proof,
                &format!("e3-chonk-multi-node-fold-verify-{}", material.party_id),
                material.party_id as u64,
                &artifacts_dir,
            )
            .expect("NodeFold verification invocation"));
        node_fold_proofs.push(node.proof);
        println!("NodeFold complete for node {}", material.party_id);
    }

    let party_ids: Vec<u64> = materials
        .iter()
        .map(|material| material.party_id as u64)
        .collect();
    let nodes_fold_slot_indices: Vec<u32> = (0..committee.h as u32).collect();
    let nodes_fold = generate_sequential_nodes_fold(
        &prover,
        &node_fold_proofs,
        &nodes_fold_slot_indices,
        committee.h,
        "e3-chonk-multi-nodes-fold",
        &artifacts_dir,
    )
    .expect("nodes_fold proof");
    assert!(prover
        .verify_fold_proof(
            &nodes_fold,
            "e3-chonk-multi-nodes-fold-verify",
            0,
            &artifacts_dir,
        )
        .expect("nodes_fold verification invocation"));
    println!("NodesFold complete");

    let threshold_shares: Vec<PublicKeyShare> = materials
        .iter()
        .map(|material| threshold_public_key_share(preset, &material.pk_generation))
        .collect();
    let aggregate_public_key = threshold_shares
        .iter()
        .cloned()
        .aggregate()
        .expect("aggregate threshold public key shares");
    let (threshold_params, _) = build_pair_for_preset(preset).expect("threshold parameters");
    let c5_data = PkAggregationCircuitData {
        committee: committee.clone(),
        public_key: aggregate_public_key,
        pk0_shares: materials
            .iter()
            .map(|material| material.pk_generation.pk0_share.clone())
            .collect(),
        a: CrtPolynomial::from_fhe_polynomial(
            &create_deterministic_crp_from_default_seed(&threshold_params).poly(),
        ),
    };
    let c5 = PkAggregationCircuit
        .prove_with_variant(
            &prover,
            &preset,
            &c5_data,
            "e3-chonk-multi-c5",
            CircuitVariant::Default,
            &artifacts_dir,
        )
        .expect("C5 proof");
    let c5_public = proof_public_fields(&c5);
    for (party_id, material) in materials.iter().enumerate() {
        assert_eq!(
            c5_public[party_id],
            proof_public_fields(&material.c1_proof)[1],
            "C5/C1 pk commitment binding for party {party_id}"
        );
    }
    println!("C5 proof complete");

    let committee_addresses: Vec<Address> = (0..committee.n)
        .map(|party_id| Address::from([party_id as u8 + 1; 20]))
        .collect();
    let c3_chunk_fold_vk = tubes.as_ref().map(|_| {
        load_vk_artifacts(
            &prover.circuits_dir(CircuitVariant::Default, &artifacts_dir),
            CircuitName::C3ChunkFold,
        )
        .expect("C3 chunk fold VK")
    });
    let c3_overrides = c3_chunk_fold_vk
        .as_ref()
        .map(|vk| DkgAggregationC3Overrides {
            c3_fold_key_hash: &vk.key_hash,
            c3ab_fold_circuit: CircuitName::C3abFold,
        });
    let dkg_aggregator = prove_dkg_aggregation(
        &prover,
        &DkgAggregationInput {
            node_fold_proofs: &node_fold_proofs,
            nodes_fold_proof: Some(&nodes_fold),
            c5_proof: &c5,
            party_ids: &party_ids,
            committee_addresses: &committee_addresses,
            c3_overrides,
        },
        "e3-chonk-multi-dkg-aggregator",
        preset,
        committee_size,
    )
    .expect("DkgAggregator proof");
    println!("DKG aggregator proof complete");
    assert!(prover
        .verify_evm_proof(
            &dkg_aggregator,
            "e3-chonk-multi-dkg-aggregator-verify",
            0,
            &artifacts_dir,
        )
        .expect("DkgAggregator verification invocation"));

    drop(temp);
}

#[tokio::test]
#[ignore = "benchmark: real Chonk C3 through nodes_fold and dkg_aggregator"]
async fn chonk_c3_flows_through_multi_node_dkg_aggregator() {
    run_multi_node_dkg_aggregator(CiphernodesCommitteeSize::Minimum, true).await;
}

#[tokio::test]
#[ignore = "benchmark: real Chonk C3 through nodes_fold and dkg_aggregator for 36 leaves/node"]
async fn chonk_c3_flows_through_small_multi_node_dkg_aggregator() {
    run_multi_node_dkg_aggregator(CiphernodesCommitteeSize::Small, true).await;
}

#[tokio::test]
#[ignore = "benchmark: classic sequential C3 through nodes_fold and dkg_aggregator for 36 leaves/node"]
async fn classic_c3_flows_through_small_multi_node_dkg_aggregator() {
    run_multi_node_dkg_aggregator(CiphernodesCommitteeSize::Small, false).await;
}
