# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-05 13:57:05 UTC

**Git Branch:** `nargo22`  
**Git Commit:** `b5b255d8040f94d0e90df3a0bab0f1dff0430f10`

**Committee Size:** `H=2`, `N=3`, `T=1`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting | Value |
|---------|-------|
| Benchmark mode | `insecure` |
| BFV preset (artifacts) | `insecure-512` |
| BFV preset (enum) | `InsecureThreshold512` |
| λ (smudging / error) | 2 |
| Nodes spawned (builder) | 7 |
| Network model | `in_process_bus` |
| Testmode harness | true |
| `proof_aggregation_enabled` | true |
| `BENCHMARK_MULTITHREAD_JOBS` (max concurrent ZK jobs) | 1 |
| Rayon worker threads | 1 |
| CPU cores (host) | 14 |
| `dkg_fold_attestation_verifier` (EIP-712) | `0x7969c5eD335650692Bc04293B07F5BF2e7A673C0` |
| Verbose logging (`run_benchmarks.sh --verbose`) | false |

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
| C0 | 6810 | 0.12 | 12.46 | 14.31 |
| C1 | 53448 | 0.28 | 12.57 | 14.31 |
| C2a | 41207 | 0.24 | 11.41 | 14.31 |
| C2b | 79554 | 0.35 | 12.60 | 14.31 |
| C3a | 120078 | 0.51 | 12.57 | 14.31 |
| C3b | 120078 | 0.51 | 12.57 | 14.31 |
| C4a | 62713 | 0.30 | 12.33 | 14.31 |
| C4b | 62713 | 0.30 | 12.33 | 14.31 |
| C5 | 21464 | 0.17 | 12.72 | 14.31 |
| user_data_encryption | 53695 | 0.29 | 12.91 | 14.31 |
| C6 | 86892 | 0.43 | 11.45 | 14.31 |
| C7 | 89602 | 0.37 | 11.79 | 14.31 |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
|----------|------------|-------------------|------------|--------------|-----------|
| Π_DKG | 10.44 KB | 0.38 KB | 3125181 | 173552 | 3298733 |
| Π_user | 14.31 KB | 0.12 KB | 2982346 | 200640 | 3182986 |
| Π_dec | 10.44 KB | 3.56 KB | 3716358 | 186896 | 3903254 |

### Role / Phase / Activity

| Role | Phase | Activity | Metric | Duration | Proof size | Bandwidth |
|------|-------|----------|--------|----------|------------|-----------|
| Each ciphernode | P1 | one-time DKG participation (test harness) | wall_clock | 189.97 s | 114.50 KB | 115.56 KB |
| Aggregator | P2 | C5 + Π_DKG fold (aggregator span) | wall_clock | 13.50 s | 10.44 KB | 10.81 KB |
| User | P3 | per user input | isolated_nargo | 0.55 s | 14.31 KB | 14.44 KB |
| Each ciphernode | P4 | per computation output (C6) | isolated_nargo | 0.43 s | 14.31 KB | 14.50 KB |
| Aggregator | P4 | C7 + Π_dec fold (full publish→aggregate) | wall_clock | 46.95 s | 10.44 KB | 14.00 KB |
| Aggregator | P4 | C7 + fold only (pending→plaintext span) | wall_clock | 41.73 s | 10.44 KB | 14.00 KB |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **4.45 s** — not comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase | Metric | Duration (s) |
|-------|--------|---------------|
| Starting trbfv actor test | `wall_clock` | 0.00 |
| Setup completed | `wall_clock` | 0.81 |
| Committee Setup Completed | `wall_clock` | 7.02 |
| Committee Finalization Complete | `wall_clock` | 0.00 |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 13.50 |
| ThresholdShares -> PublicKeyAggregated | `wall_clock` | 189.97 |
| E3Request -> PublicKeyAggregated | `wall_clock` | 190.47 |
| Application CT Gen | `wall_clock` | 0.01 |
| Running FHE Application | `wall_clock` | 0.00 |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall) | `wall_clock` | 41.73 |
| Ciphertext published -> PlaintextAggregated | `wall_clock` | 46.95 |
| Entire Test | `wall_clock` | 245.26 |

### Multithread job timings (`tracked_job_wall`)

| Name | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| CalculateDecryptionKey | 0.00 | 3 | 0.01 |
| CalculateDecryptionShare | 0.02 | 3 | 0.06 |
| CalculateThresholdDecryption | 0.02 | 1 | 0.02 |
| GenEsiSss | 0.00 | 3 | 0.01 |
| GenPkShareAndSkSss | 0.01 | 3 | 0.03 |
| NodeDkgFold/c2ab_fold | 3.64 | 3 | 10.91 |
| NodeDkgFold/c3a_fold | 16.20 | 3 | 48.60 |
| NodeDkgFold/c3ab_fold | 3.54 | 3 | 10.63 |
| NodeDkgFold/c3b_fold | 16.10 | 3 | 48.29 |
| NodeDkgFold/c4ab_fold | 3.63 | 3 | 10.90 |
| NodeDkgFold/node_fold | 8.39 | 3 | 25.18 |
| ZkDecryptedSharesAggregation | 1.49 | 1 | 1.49 |
| ZkDecryptionAggregation | 40.22 | 1 | 40.22 |
| ZkDkgAggregation | 4.30 | 1 | 4.30 |
| ZkDkgShareDecryption | 0.35 | 6 | 2.12 |
| ZkNodeDkgFold | 51.51 | 3 | 154.52 |
| ZkNodesFoldStep | 4.52 | 2 | 9.04 |
| ZkPkAggregation | 0.16 | 1 | 0.16 |
| ZkPkBfv | 0.13 | 3 | 0.38 |
| ZkPkGeneration | 0.38 | 3 | 1.13 |
| ZkShareComputation | 0.37 | 6 | 2.23 |
| ZkShareEncryption | 0.65 | 24 | 15.51 |
| ZkThresholdShareDecryption | 1.68 | 3 | 5.04 |
| ZkVerifyShareDecryptionProofs | 0.03 | 3 | 0.08 |
| ZkVerifyShareProofs | 0.08 | 5 | 0.41 |

Sum of tracked job wall time: **391.30 s** — **not** end-to-end latency (jobs run in parallel up to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| c2ab_fold | 3.64 | 3 | 10.91 |
| c3a_fold | 16.20 | 3 | 48.60 |
| c3ab_fold | 3.54 | 3 | 10.63 |
| c3b_fold | 16.10 | 3 | 48.29 |
| c4ab_fold | 3.63 | 3 | 10.90 |
| node_fold | 8.39 | 3 | 25.18 |

### Aggregation jobs (`tracked_job_wall`)

| Operation | Avg (s) | Runs | Total (s) |
|-----------|---------|------|-----------|
| ZkDecryptedSharesAggregation | 1.49 | 1 | 1.49 |
| ZkDecryptionAggregation | 40.22 | 1 | 40.22 |
| ZkDkgAggregation | 4.30 | 1 | 4.30 |
| ZkNodeDkgFold | 51.51 | 3 | 154.52 |
| ZkPkAggregation | 0.16 | 1 | 0.16 |

Sum of aggregation job tracked time: **200.69 s** (parallel CPU work; not P1/P2 wall clock).

### Folded on-chain artifacts (exported for Π_DKG / Π_dec gas)

| Artifact | Proof (bytes) | Public inputs (bytes) |
|----------|---------------|------------------------|
| dkg_aggregator | 10688 | 384 |
| decryption_aggregator | 10688 | 3648 |

## Raw circuit benchmark JSON (Nargo)

Source files for the **Circuit Benchmarks** table. Persist this directory with `crisp_verify_gas.json` (and optional `integration_summary.json`) to regenerate the report without re-running the integration test.

| File |
|------|
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
