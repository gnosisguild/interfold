# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-06 10:23:27 UTC

**Git Branch:** `chunk-circuits`  
**Git Commit:** `ee831b996d23af4d6ae9a04bb66e830a08d8effe`

**Committee Size:** `H=2`, `N=3`, `T=1`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting | Value |
|---------|-------|
| Benchmark mode | `secure` |
| BFV preset (artifacts) | `secure-8192` |
| BFV preset (enum) | `SecureThreshold8192` |
| λ (smudging / error) | 50 |
| Nodes spawned (builder) | 7 |
| Network model | `in_process_bus` |
| Testmode harness | true |
| `proof_aggregation_enabled` | true |
| `BENCHMARK_MULTITHREAD_JOBS` (max concurrent ZK jobs) | 13 |
| Rayon worker threads | 13 |
| CPU cores (host) | 14 |
| `dkg_fold_attestation_verifier` (EIP-712) | `0x7969c5eD335650692Bc04293B07F5BF2e7A673C0` |
| Verbose logging (`run_benchmarks.sh --verbose`) | true |

### Hardware & software (Nargo / Barretenberg host)

| | |
|--|--|
| **CPU** | Apple M4 Pro |
| **CPU cores** | 14 |
| **RAM** | 48.00 GB |
| **OS** | Darwin |
| **Architecture** | arm64 |
| **Nargo** | nargo version = 1.0.0-beta.26 noirc version = 1.0.0-beta.26+40d6574f851d926f93e0c3a271bac3e6e82ac905 (git version hash: 40d6574f851d926f93e0c3a271bac3e6e82ac905, is dirty: false)  |
| **Barretenberg** | 5.1.0  |

---

## Audit status

On-chain verify gas: **complete** (CRISP Π_user + Interfold Π_DKG / Π_dec replay).

---

## Measurement methodology

| Metric kind | Source | Meaning | Do **not** use for |
|-------------|--------|---------|-------------------|
| **wall_clock** | `test_trbfv_actor` phase timers / HLC event span | End-to-end wait in the in-process test harness | Production WAN latency; per-node deployment cost |
| **isolated_nargo** | `benchmark_circuit.sh` per circuit | Single `bb prove` on oracle witness, one circuit at a time | Full protocol pipeline (different witness path) |
| **tracked_job_wall** | `MultithreadReport` per `ComputeRequest` | Wall time of each job on the shared Rayon pool (≤ `BENCHMARK_MULTITHREAD_JOBS` concurrent) | End-to-end time — **sums exceed wall clock** when jobs overlap |

**Harness limits (integration):** all ciphernodes share one process and bus (`network_model: in_process_bus`); sortition registers extra nodes; `testmode_*` enabled; proof aggregation always enabled. Compare runs only with the same `benchmark_mode`, committee, `BENCHMARK_MULTITHREAD_JOBS`, commit, and hardware.

---
## Protocol Summary

### Circuit Benchmarks (isolated Nargo + Barretenberg)

Single-circuit `bb prove` on the benchmark oracle witness (not the integration actor pipeline).

| Circuit | Constraints | Prove (s) | Verify (ms) | Proof (KB) |
|---------|-------------|-----------|-------------|------------|
| C0 | 287727 | 1.02 | 11.38 | 14.31 |
| C1 | 2223114 | 6.23 | 11.37 | 14.31 |
| C2a | 1446311 | 4.04 | 11.18 | 14.31 |
| C2b | 2888964 | 7.50 | 11.28 | 14.31 |
| C3a | 3475203 | 9.20 | 11.36 | 14.31 |
| C3b | 3475203 | 9.20 | 11.36 | 14.31 |
| C4a | 1746030 | 4.77 | 11.72 | 14.31 |
| C4b | 1746030 | 4.77 | 11.72 | 14.31 |
| C5 | 754560 | 2.49 | 12.30 | 14.31 |
| user_data_encryption | 1688639 | 5.01 | 11.59 | 14.31 |
| C6 | 2977228 | 8.38 | 11.36 | 14.31 |
| C7 | 108461 | 0.44 | 11.72 | 14.31 |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
|----------|------------|-------------------|------------|--------------|-----------|
| Π_DKG | 10.44 KB | 0.38 KB | 3125242 | 173612 | 3298854 |
| Π_user | 14.31 KB | 0.12 KB | 2982238 | 200544 | 3182782 |
| Π_dec | 10.44 KB | 3.56 KB | 3716492 | 187028 | 3903520 |

### Role / Phase / Activity

| Role | Phase | Activity | Metric | Duration | Proof size | Bandwidth |
|------|-------|----------|--------|----------|------------|-----------|
| Each ciphernode | P1 | one-time DKG participation (test harness) | wall_clock | 928.83 s | 114.50 KB | 115.88 KB |
| Aggregator | P2 | C5 + Π_DKG fold (aggregator span) | wall_clock | 128.88 s | 10.44 KB | 10.81 KB |
| User | P3 | per user input | isolated_nargo | 9.15 s | 14.31 KB | 14.44 KB |
| Each ciphernode | P4 | per computation output (C6) | isolated_nargo | 8.38 s | 14.31 KB | 14.50 KB |
| Aggregator | P4 | C7 + Π_dec fold (full publish→aggregate) | wall_clock | 128.98 s | 10.44 KB | 14.00 KB |
| Aggregator | P4 | C7 + fold only (pending→plaintext span) | wall_clock | 40.68 s | 10.44 KB | 14.00 KB |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **17.27 s** — not comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase | Metric | Duration (s) |
|-------|--------|---------------|
| Starting trbfv actor test | `wall_clock` | 0.00 |
| Setup completed | `wall_clock` | 0.90 |
| Committee Setup Completed | `wall_clock` | 7.03 |
| Committee Finalization Complete | `wall_clock` | 0.01 |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 128.88 |
| ThresholdShares -> PublicKeyAggregated | `wall_clock` | 928.83 |
| E3Request -> PublicKeyAggregated | `wall_clock` | 929.34 |
| Application CT Gen | `wall_clock` | 0.29 |
| Running FHE Application | `wall_clock` | 0.00 |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall) | `wall_clock` | 40.68 |
| Ciphertext published -> PlaintextAggregated | `wall_clock` | 128.98 |
| Entire Test | `wall_clock` | 1066.55 |

### Multithread job timings (`tracked_job_wall`)

| Name | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| CalculateDecryptionKey | 0.04 | 3 | 0.11 |
| CalculateDecryptionShare | 0.17 | 3 | 0.51 |
| CalculateThresholdDecryption | 0.15 | 1 | 0.15 |
| GenEsiSss | 0.08 | 3 | 0.25 |
| GenPkShareAndSkSss | 0.10 | 3 | 0.30 |
| NodeDkgFold/c2ab_chunk_fold | 14.57 | 3 | 43.72 |
| NodeDkgFold/c3a_fold | 89.50 | 3 | 268.51 |
| NodeDkgFold/c3ab_fold | 7.72 | 3 | 23.16 |
| NodeDkgFold/c3b_fold | 89.69 | 3 | 269.08 |
| NodeDkgFold/c4ab_fold | 9.46 | 3 | 28.37 |
| NodeDkgFold/node_fold | 13.84 | 3 | 41.52 |
| ZkDecryptedSharesAggregation | 2.65 | 1 | 2.65 |
| ZkDecryptionAggregation | 37.93 | 1 | 37.93 |
| ZkDkgAggregation | 3.97 | 1 | 3.97 |
| ZkDkgShareDecryption | 17.99 | 6 | 107.96 |
| ZkNodeDkgFold | 120.93 | 3 | 362.79 |
| ZkNodesFoldStep | 4.25 | 2 | 8.49 |
| ZkPkAggregation | 13.29 | 1 | 13.29 |
| ZkPkBfv | 2.64 | 3 | 7.93 |
| ZkPkGeneration | 320.92 | 3 | 962.75 |
| ZkShareComputation | 602.91 | 6 | 3617.45 |
| ZkShareEncryption | 131.41 | 36 | 4730.61 |
| ZkThresholdShareDecryption | 86.76 | 3 | 260.27 |
| ZkVerifyShareDecryptionProofs | 0.05 | 3 | 0.14 |
| ZkVerifyShareProofs | 0.14 | 5 | 0.72 |

Sum of tracked job wall time: **10792.65 s** — **not** end-to-end latency (jobs run in parallel up to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| c2ab_chunk_fold | 14.57 | 3 | 43.72 |
| c3a_fold | 89.50 | 3 | 268.51 |
| c3ab_fold | 7.72 | 3 | 23.16 |
| c3b_fold | 89.69 | 3 | 269.08 |
| c4ab_fold | 9.46 | 3 | 28.37 |
| node_fold | 13.84 | 3 | 41.52 |

### Aggregation jobs (`tracked_job_wall`)

| Operation | Avg (s) | Runs | Total (s) |
|-----------|---------|------|-----------|
| ZkDecryptedSharesAggregation | 2.65 | 1 | 2.65 |
| ZkDecryptionAggregation | 37.93 | 1 | 37.93 |
| ZkDkgAggregation | 3.97 | 1 | 3.97 |
| ZkNodeDkgFold | 120.93 | 3 | 362.79 |
| ZkPkAggregation | 13.29 | 1 | 13.29 |

Sum of aggregation job tracked time: **420.63 s** (parallel CPU work; not P1/P2 wall clock).

### Folded on-chain artifacts (exported for Π_DKG / Π_dec gas)

| Artifact | Proof (bytes) | Public inputs (bytes) |
|----------|---------------|------------------------|
| dkg_aggregator | 10688 | 384 |
| decryption_aggregator | 10688 | 3648 |

## Raw circuit benchmark JSON (Nargo)

Source files for the **Circuit Benchmarks** table. Persist this directory with `crisp_verify_gas.json` (and optional `integration_summary.json`) to regenerate the report without re-running the integration test.

| File |
|------|
| `config_default.json` |
| `dkg_e_sm_share_computation_default.json` |
| `dkg_pk_default.json` |
| `dkg_share_decryption_default.json` |
| `dkg_share_encryption_default.json` |
| `dkg_sk_share_computation_default.json` |
| `threshold_decrypted_shares_aggregation_default.json` |
| `threshold_pk_aggregation_default.json` |
| `threshold_pk_generation_default.json` |
| `threshold_share_decryption_default.json` |
| `threshold_user_data_encryption_ct0_default.json` |
| `threshold_user_data_encryption_ct1_default.json` |

## Notes

- All nodes are executed on the same machine in this benchmark run, so inter-node network latency is effectively 0.
