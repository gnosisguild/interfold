// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Real C3 leaf and sequential-fold timing used by the Chonk comparison probe.
//!
//! This is intentionally ignored: it invokes several real BFV proofs and is a benchmark, not a
//! regression test. Run it explicitly with `--ignored --nocapture` after building the insecure
//! minimum circuit artifacts.

mod common;

use std::time::Instant;

use common::{
    find_bb, setup_compiled_circuit, setup_recursive_aggregation_fold_circuit, setup_test_prover,
};
use e3_fhe_params::BfvPreset;
use e3_zk_helpers::computation::DkgInputType;
use e3_zk_helpers::dkg::share_encryption::{ShareEncryptionCircuit, ShareEncryptionCircuitData};
use e3_zk_helpers::CiphernodesCommitteeSize;
use e3_zk_prover::{generate_sequential_c3_fold, CircuitVariant, Provable, ZkProver};

#[tokio::test]
#[ignore = "benchmark: generates real C3 leaf and sequential fold proofs"]
async fn real_c3_sequential_fold_benchmark() {
    let Some(bb) = find_bb().await else {
        panic!("bb binary is required for the benchmark");
    };

    let (backend, temp) = setup_test_prover(&bb).await;
    setup_compiled_circuit(&backend, "dkg", "share_encryption").await;
    setup_recursive_aggregation_fold_circuit(&backend, e3_events::CircuitName::C3Fold).await;
    setup_recursive_aggregation_fold_circuit(&backend, e3_events::CircuitName::C3FoldKernel).await;

    let preset = BfvPreset::InsecureThreshold512;
    let committee = CiphernodesCommitteeSize::Minimum.values();
    let search_defaults = preset.search_defaults().expect("insecure search defaults");
    let slots = [0u32, 2u32, 4u32];
    let mut samples = Vec::with_capacity(slots.len());

    for slot in slots {
        let mut sample = ShareEncryptionCircuitData::generate_sample(
            preset,
            committee.clone(),
            DkgInputType::SecretKey,
            search_defaults.z,
        )
        .expect("real ShareEncryption sample");
        sample.party_idx = slot / 2;
        sample.mod_idx = slot % 2;
        samples.push(sample);
    }

    let prover = ZkProver::new(&backend);
    let artifacts_dir = preset.artifacts_dir_for_committee("minimum");
    let leaf_started = Instant::now();
    let mut inner_proofs = Vec::with_capacity(samples.len());

    for (index, sample) in samples.iter().enumerate() {
        inner_proofs.push(
            ShareEncryptionCircuit
                .prove_with_variant(
                    &prover,
                    &preset,
                    sample,
                    &format!("chonk-c3-leaf-{index}"),
                    CircuitVariant::Recursive,
                    &artifacts_dir,
                )
                .expect("real C3 leaf proof"),
        );
    }

    let leaf_seconds = leaf_started.elapsed().as_secs_f64();
    let fold_started = Instant::now();
    let folded = generate_sequential_c3_fold(
        &prover,
        &inner_proofs,
        &slots,
        6,
        "chonk-c3-sequential-fold",
        &artifacts_dir,
    )
    .expect("sequential C3 fold");
    let fold_seconds = fold_started.elapsed().as_secs_f64();

    assert!(prover
        .verify_fold_proof(&folded, "chonk-c3-sequential-fold", 1, &artifacts_dir)
        .expect("sequential C3 verification invocation"));

    println!("real C3 leaf proofs: {}", inner_proofs.len());
    println!("real C3 leaf proving time: {leaf_seconds:.2}s");
    println!("sequential C3 fold time: {fold_seconds:.2}s");
    println!(
        "sequential C3 total time: {:.2}s",
        leaf_seconds + fold_seconds
    );

    for index in 0..inner_proofs.len() {
        prover
            .cleanup(&format!("chonk-c3-leaf-{index}"))
            .expect("leaf work cleanup");
    }
    prover
        .cleanup("chonk-c3-sequential-fold")
        .expect("fold work cleanup");
    drop(temp);
}
