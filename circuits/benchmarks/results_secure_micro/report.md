# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-07 13:23:51 UTC

**Git Branch:** `chunk-circuits`  
**Git Commit:** `cf4341350d0d9d5b1fbb6042228db359b1dfe70e`

**Committee Size:** `H=5`, `N=9`, `T=4`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting                                               | Value                                        |
| ----------------------------------------------------- | -------------------------------------------- |
| Benchmark mode                                        | `secure`                                     |
| BFV preset (artifacts)                                | `secure-8192`                                |
| BFV preset (enum)                                     | `SecureThreshold8192`                        |
| λ (smudging / error)                                  | 50                                           |
| Nodes spawned (builder)                               | 16                                           |
| Network model                                         | `in_process_bus`                             |
| Testmode harness                                      | true                                         |
| `proof_aggregation_enabled`                           | true                                         |
| `BENCHMARK_MULTITHREAD_JOBS` (max concurrent ZK jobs) | 13                                           |
| Rayon worker threads                                  | 13                                           |
| CPU cores (host)                                      | 14                                           |
| `dkg_fold_attestation_verifier` (EIP-712)             | `0x7969c5eD335650692Bc04293B07F5BF2e7A673C0` |
| Verbose logging (`run_benchmarks.sh --verbose`)       | true                                         |

### Hardware & software (Nargo / Barretenberg host)

|                  |                                                                                                                                                                                    |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **CPU**          | Apple M4 Pro                                                                                                                                                                       |
| **CPU cores**    | 14                                                                                                                                                                                 |
| **RAM**          | 48.00 GB                                                                                                                                                                           |
| **OS**           | Darwin                                                                                                                                                                             |
| **Architecture** | arm64                                                                                                                                                                              |
| **Nargo**        | nargo version = 1.0.0-beta.26 noirc version = 1.0.0-beta.26+40d6574f851d926f93e0c3a271bac3e6e82ac905 (git version hash: 40d6574f851d926f93e0c3a271bac3e6e82ac905, is dirty: false) |
| **Barretenberg** | 5.1.0                                                                                                                                                                              |

---

## Audit status

On-chain verify gas: **complete** (CRISP Π_user + Interfold Π_DKG / Π_dec replay).

---

## Measurement methodology

| Metric kind          | Source                                           | Meaning                                                                                    | Do **not** use for                                             |
| -------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| **wall_clock**       | `test_trbfv_actor` phase timers / HLC event span | End-to-end wait in the in-process test harness                                             | Production WAN latency; per-node deployment cost               |
| **isolated_nargo**   | `benchmark_circuit.sh` per circuit               | Single `bb prove` on oracle witness, one circuit at a time                                 | Full protocol pipeline (different witness path)                |
| **tracked_job_wall** | `MultithreadReport` per `ComputeRequest`         | Wall time of each job on the shared Rayon pool (≤ `BENCHMARK_MULTITHREAD_JOBS` concurrent) | End-to-end time — **sums exceed wall clock** when jobs overlap |

**Harness limits (integration):** all ciphernodes share one process and bus
(`network_model: in_process_bus`); sortition registers extra nodes; `testmode_*` enabled; proof
aggregation always enabled. Compare runs only with the same `benchmark_mode`, committee,
`BENCHMARK_MULTITHREAD_JOBS`, commit, and hardware.

---

## Protocol Summary

### Circuit Benchmarks (isolated Nargo + Barretenberg)

Single-circuit `bb prove` on the benchmark oracle witness (not the integration actor pipeline).

| Circuit              | Constraints | Prove (s) | Verify (ms) | Proof (KB) |
| -------------------- | ----------- | --------- | ----------- | ---------- |
| C0                   | 287727      | 1.04      | 11.75       | 14.31      |
| C1                   | 2226972     | 6.04      | 11.20       | 14.31      |
| C2a                  | 4331713     | 11.08     | 11.15       | 14.31      |
| C2b                  | 5773850     | 14.31     | 11.25       | 14.31      |
| C3a                  | 3478343     | 9.20      | 11.20       | 14.31      |
| C3b                  | 3478343     | 9.20      | 11.20       | 14.31      |
| C4a                  | 2447701     | 6.61      | 11.40       | 14.31      |
| C4b                  | 2447701     | 6.61      | 11.40       | 14.31      |
| C5                   | 1426371     | 4.18      | 10.99       | 14.31      |
| user_data_encryption | 1688639     | 4.81      | 11.06       | 14.31      |
| C6                   | 2977228     | 8.00      | 11.23       | 14.31      |
| C7                   | 191201      | 0.66      | 11.48       | 14.31      |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
| -------- | ---------- | ----------------- | ---------- | ------------ | --------- |
| Π_DKG    | 10.44 KB   | 0.72 KB           | 3148210    | 177912       | 3326122   |
| Π_user   | 14.31 KB   | 0.12 KB           | 2982166    | 200712       | 3182878   |
| Π_dec    | 10.44 KB   | 3.84 KB           | 3761763    | 190508       | 3952271   |

### Role / Phase / Activity

| Role            | Phase | Activity                                  | Metric         | Duration  | Proof size | Bandwidth |
| --------------- | ----- | ----------------------------------------- | -------------- | --------- | ---------- | --------- |
| Each ciphernode | P1    | one-time DKG participation (test harness) | wall_clock     | 5333.62 s | 114.50 KB  | 117.69 KB |
| Aggregator      | P2    | C5 + Π_DKG fold (aggregator span)         | wall_clock     | 385.01 s  | 10.44 KB   | 11.16 KB  |
| User            | P3    | per user input                            | isolated_nargo | 8.93 s    | 14.31 KB   | 14.44 KB  |
| Each ciphernode | P4    | per computation output (C6)               | isolated_nargo | 8.00 s    | 14.31 KB   | 14.50 KB  |
| Aggregator      | P4    | C7 + Π_dec fold (full publish→aggregate)  | wall_clock     | 286.74 s  | 10.44 KB   | 14.28 KB  |
| Aggregator      | P4    | C7 + fold only (pending→plaintext span)   | wall_clock     | 75.92 s   | 10.44 KB   | 14.28 KB  |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **38.43 s** — not
comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase                                                              | Metric       | Duration (s) |
| ------------------------------------------------------------------ | ------------ | ------------ |
| Starting trbfv actor test                                          | `wall_clock` | 0.00         |
| Setup completed                                                    | `wall_clock` | 1.99         |
| Committee Setup Completed                                          | `wall_clock` | 16.07        |
| Committee Finalization Complete                                    | `wall_clock` | 0.00         |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 385.01       |
| ThresholdShares -> PublicKeyAggregated                             | `wall_clock` | 5333.62      |
| E3Request -> PublicKeyAggregated                                   | `wall_clock` | 5334.12      |
| Application CT Gen                                                 | `wall_clock` | 0.29         |
| Running FHE Application                                            | `wall_clock` | 0.00         |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall)   | `wall_clock` | 75.92        |
| Ciphertext published -> PlaintextAggregated                        | `wall_clock` | 286.74       |
| Entire Test                                                        | `wall_clock` | 5639.23      |

### Multithread job timings (`tracked_job_wall`)

| Name                          | Avg (s) | Runs | Total (s) |
| ----------------------------- | ------- | ---- | --------- |
| CalculateDecryptionKey        | 0.05    | 9    | 0.43      |
| CalculateDecryptionShare      | 0.17    | 9    | 1.53      |
| CalculateThresholdDecryption  | 0.21    | 1    | 0.21      |
| GenEsiSss                     | 36.61   | 9    | 329.52    |
| GenPkShareAndSkSss            | 0.79    | 9    | 7.08      |
| NodeDkgFold/c2ab_chunk_fold   | 25.97   | 9    | 233.77    |
| NodeDkgFold/c3a_fold          | 627.37  | 9    | 5646.33   |
| NodeDkgFold/c3ab_fold         | 11.60   | 9    | 104.41    |
| NodeDkgFold/c3b_fold          | 588.47  | 9    | 5296.26   |
| NodeDkgFold/c4ab_fold         | 11.32   | 9    | 101.84    |
| NodeDkgFold/node_fold         | 26.07   | 9    | 234.65    |
| ZkDecryptedSharesAggregation  | 5.00    | 1    | 5.00      |
| ZkDecryptionAggregation       | 70.69   | 1    | 70.69     |
| ZkDkgAggregation              | 4.05    | 1    | 4.05      |
| ZkDkgShareDecryption          | 60.97   | 18   | 1097.45   |
| ZkNodeDkgFold                 | 876.10  | 9    | 7884.86   |
| ZkNodesFoldStep               | 3.66    | 5    | 18.31     |
| ZkPkAggregation               | 34.38   | 1    | 34.38     |
| ZkPkBfv                       | 6.62    | 9    | 59.57     |
| ZkPkGeneration                | 108.72  | 9    | 978.50    |
| ZkShareComputation            | 231.89  | 18   | 4173.96   |
| ZkShareEncryption             | 105.74  | 432  | 45680.39  |
| ZkThresholdShareDecryption    | 163.96  | 9    | 1475.68   |
| ZkVerifyShareDecryptionProofs | 0.18    | 9    | 1.61      |
| ZkVerifyShareProofs           | 0.77    | 11   | 8.47      |

Sum of tracked job wall time: **73448.97 s** — **not** end-to-end latency (jobs run in parallel up
to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step            | Avg (s) | Runs | Total (s) |
| --------------- | ------- | ---- | --------- |
| c2ab_chunk_fold | 25.97   | 9    | 233.77    |
| c3a_fold        | 627.37  | 9    | 5646.33   |
| c3ab_fold       | 11.60   | 9    | 104.41    |
| c3b_fold        | 588.47  | 9    | 5296.26   |
| c4ab_fold       | 11.32   | 9    | 101.84    |
| node_fold       | 26.07   | 9    | 234.65    |

### Aggregation jobs (`tracked_job_wall`)

| Operation                    | Avg (s) | Runs | Total (s) |
| ---------------------------- | ------- | ---- | --------- |
| ZkDecryptedSharesAggregation | 5.00    | 1    | 5.00      |
| ZkDecryptionAggregation      | 70.69   | 1    | 70.69     |
| ZkDkgAggregation             | 4.05    | 1    | 4.05      |
| ZkNodeDkgFold                | 876.10  | 9    | 7884.86   |
| ZkPkAggregation              | 34.38   | 1    | 34.38     |

Sum of aggregation job tracked time: **7998.98 s** (parallel CPU work; not P1/P2 wall clock).

### Folded on-chain artifacts (exported for Π_DKG / Π_dec gas)

| Artifact              | Proof (bytes) | Public inputs (bytes) |
| --------------------- | ------------- | --------------------- |
| dkg_aggregator        | 10688         | 736                   |
| decryption_aggregator | 10688         | 3936                  |

## Raw circuit benchmark JSON (Nargo)

Source files for the **Circuit Benchmarks** table. Persist this directory with
`crisp_verify_gas.json` (and optional `integration_summary.json`) to regenerate the report without
re-running the integration test.

| File                                                  |
| ----------------------------------------------------- |
| `config_default.json`                                 |
| `dkg_e_sm_share_computation_default.json`             |
| `dkg_pk_default.json`                                 |
| `dkg_share_decryption_default.json`                   |
| `dkg_share_encryption_default.json`                   |
| `dkg_sk_share_computation_default.json`               |
| `threshold_decrypted_shares_aggregation_default.json` |
| `threshold_pk_aggregation_default.json`               |
| `threshold_pk_generation_default.json`                |
| `threshold_share_decryption_default.json`             |
| `threshold_user_data_encryption_ct0_default.json`     |
| `threshold_user_data_encryption_ct1_default.json`     |

## Notes

- All nodes are executed on the same machine in this benchmark run, so inter-node network latency is
  effectively 0.
