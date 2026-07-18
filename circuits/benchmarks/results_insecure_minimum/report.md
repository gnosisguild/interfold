# Interfold ZK Circuit Benchmarks

**Generated:** 2026-07-18 21:04:24 UTC

**Git Branch:** `circuits-audits`  
**Git Commit:** `135c1344a6c160a49d0086c9cf9574e66e6df8aa`

**Committee Size:** `H=2`, `N=3`, `T=1`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting                                               | Value                                        |
| ----------------------------------------------------- | -------------------------------------------- |
| Benchmark mode                                        | `insecure`                                   |
| BFV preset (artifacts)                                | `insecure-512`                               |
| BFV preset (enum)                                     | `InsecureThreshold512`                       |
| λ (smudging / error)                                  | 2                                            |
| Nodes spawned (builder)                               | 7                                            |
| Network model                                         | `in_process_bus`                             |
| Testmode harness                                      | true                                         |
| `proof_aggregation_enabled`                           | true                                         |
| `BENCHMARK_MULTITHREAD_JOBS` (max concurrent ZK jobs) | 13                                           |
| Rayon worker threads                                  | 13                                           |
| CPU cores (host)                                      | 14                                           |
| `dkg_fold_attestation_verifier` (EIP-712)             | `0x7969c5eD335650692Bc04293B07F5BF2e7A673C0` |
| Verbose logging (`run_benchmarks.sh --verbose`)       | false                                        |

### Hardware & software (Nargo / Barretenberg host)

|                  |                                                                                                                                                                                    |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **CPU**          | Apple M4 Pro                                                                                                                                                                       |
| **CPU cores**    | 14                                                                                                                                                                                 |
| **RAM**          | 48.00 GB                                                                                                                                                                           |
| **OS**           | Darwin                                                                                                                                                                             |
| **Architecture** | arm64                                                                                                                                                                              |
| **Nargo**        | nargo version = 1.0.0-beta.16 noirc version = 1.0.0-beta.16+2d46fca7203545cbbfb31a0d0328de6c10a8db95 (git version hash: 2d46fca7203545cbbfb31a0d0328de6c10a8db95, is dirty: false) |
| **Barretenberg** | 3.0.0-nightly.20260102                                                                                                                                                             |

---

## Audit status

> **Incomplete on-chain verify gas:** 1 of 3 artifact verify-gas values are **N/A**. Re-run
> `./run_benchmarks.sh` and ensure `extract_crisp_verify_gas.sh` completes (CRISP test +
> `test_trbfv_actor` + EVM replay). Calldata gas alone is not sufficient for audit sign-off.

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
| C0                   | 6847        | 0.12      | 24.33       | 15.88      |
| C1                   | 53485       | 0.32      | 24.60       | 15.88      |
| C2a                  | 41244       | 0.30      | 24.55       | 15.88      |
| C2b                  | 79591       | 0.51      | 25.52       | 15.88      |
| C3a                  | 120114      | 0.57      | 25.36       | 15.88      |
| C3b                  | 120114      | 0.57      | 25.36       | 15.88      |
| C4a                  | 62750       | 0.34      | 24.95       | 15.88      |
| C4b                  | 62750       | 0.34      | 24.95       | 15.88      |
| C5                   | 21501       | 0.21      | 24.75       | 15.88      |
| user_data_encryption | 53732       | 0.33      | 25.75       | 15.88      |
| C6                   | 86929       | 0.52      | 24.61       | 15.88      |
| C7                   | 90889       | 0.47      | 25.52       | 15.88      |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
| -------- | ---------- | ----------------- | ---------- | ------------ | --------- |
| Π_DKG    | 10.69 KB   | 0.38 KB           | 3119505    | 175020       | 3294525   |
| Π_user   | 15.88 KB   | 0.12 KB           | N/A        | 170260       | N/A       |
| Π_dec    | 10.69 KB   | 3.53 KB           | 3672202    | 187900       | 3860102   |

### Role / Phase / Activity

| Role            | Phase | Activity                                  | Metric         | Duration | Proof size | Bandwidth |
| --------------- | ----- | ----------------------------------------- | -------------- | -------- | ---------- | --------- |
| Each ciphernode | P1    | one-time DKG participation (test harness) | wall_clock     | 137.63 s | 127.00 KB  | 128.06 KB |
| Aggregator      | P2    | C5 + Π_DKG fold (aggregator span)         | wall_clock     | 125.47 s | 10.69 KB   | 11.06 KB  |
| User            | P3    | per user input                            | isolated_nargo | 0.65 s   | 15.88 KB   | 16.00 KB  |
| Each ciphernode | P4    | per computation output (C6)               | isolated_nargo | 0.52 s   | 15.88 KB   | 16.06 KB  |
| Aggregator      | P4    | C7 + Π_dec fold (full publish→aggregate)  | wall_clock     | 55.07 s  | 10.69 KB   | 14.22 KB  |
| Aggregator      | P4    | C7 + fold only (pending→plaintext span)   | wall_clock     | 51.29 s  | 10.69 KB   | 14.22 KB  |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **6.03 s** — not
comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase                                                              | Metric       | Duration (s) |
| ------------------------------------------------------------------ | ------------ | ------------ |
| Starting trbfv actor test                                          | `wall_clock` | 0.00         |
| Setup completed                                                    | `wall_clock` | 1.02         |
| Committee Setup Completed                                          | `wall_clock` | 7.02         |
| Committee Finalization Complete                                    | `wall_clock` | 0.00         |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 125.47       |
| ThresholdShares -> PublicKeyAggregated                             | `wall_clock` | 137.63       |
| E3Request -> PublicKeyAggregated                                   | `wall_clock` | 138.13       |
| Application CT Gen                                                 | `wall_clock` | 0.01         |
| Running FHE Application                                            | `wall_clock` | 0.00         |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall)   | `wall_clock` | 51.29        |
| Ciphertext published -> PlaintextAggregated                        | `wall_clock` | 55.07        |
| Entire Test                                                        | `wall_clock` | 201.25       |

### Multithread job timings (`tracked_job_wall`)

| Name                          | Avg (s) | Runs | Total (s) |
| ----------------------------- | ------- | ---- | --------- |
| CalculateDecryptionKey        | 0.00    | 3    | 0.01      |
| CalculateDecryptionShare      | 0.02    | 3    | 0.07      |
| CalculateThresholdDecryption  | 0.02    | 1    | 0.02      |
| GenEsiSss                     | 0.01    | 3    | 0.02      |
| GenPkShareAndSkSss            | 0.01    | 3    | 0.03      |
| NodeDkgFold/c2ab_fold         | 18.82   | 3    | 56.46     |
| NodeDkgFold/c3a_fold          | 73.49   | 3    | 220.47    |
| NodeDkgFold/c3ab_fold         | 7.85    | 3    | 23.56     |
| NodeDkgFold/c3b_fold          | 73.17   | 3    | 219.52    |
| NodeDkgFold/c4ab_fold         | 8.28    | 3    | 24.84     |
| NodeDkgFold/node_fold         | 18.94   | 3    | 56.82     |
| ZkDecryptedSharesAggregation  | 1.59    | 1    | 1.59      |
| ZkDecryptionAggregation       | 49.68   | 1    | 49.68     |
| ZkDkgAggregation              | 5.74    | 1    | 5.74      |
| ZkDkgShareDecryption          | 0.95    | 6    | 5.69      |
| ZkNodeDkgFold                 | 108.81  | 3    | 326.44    |
| ZkNodesFoldStep               | 5.31    | 2    | 10.63     |
| ZkPkAggregation               | 0.30    | 1    | 0.30      |
| ZkPkBfv                       | 0.23    | 3    | 0.69      |
| ZkPkGeneration                | 3.61    | 3    | 10.82     |
| ZkShareComputation            | 2.62    | 6    | 15.70     |
| ZkShareEncryption             | 3.67    | 24   | 88.14     |
| ZkThresholdShareDecryption    | 3.49    | 3    | 10.48     |
| ZkVerifyShareDecryptionProofs | 0.08    | 3    | 0.25      |
| ZkVerifyShareProofs           | 0.24    | 5    | 1.22      |

Sum of tracked job wall time: **1129.20 s** — **not** end-to-end latency (jobs run in parallel up to
`BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step      | Avg (s) | Runs | Total (s) |
| --------- | ------- | ---- | --------- |
| c2ab_fold | 18.82   | 3    | 56.46     |
| c3a_fold  | 73.49   | 3    | 220.47    |
| c3ab_fold | 7.85    | 3    | 23.56     |
| c3b_fold  | 73.17   | 3    | 219.52    |
| c4ab_fold | 8.28    | 3    | 24.84     |
| node_fold | 18.94   | 3    | 56.82     |

### Aggregation jobs (`tracked_job_wall`)

| Operation                    | Avg (s) | Runs | Total (s) |
| ---------------------------- | ------- | ---- | --------- |
| ZkDecryptedSharesAggregation | 1.59    | 1    | 1.59      |
| ZkDecryptionAggregation      | 49.68   | 1    | 49.68     |
| ZkDkgAggregation             | 5.74    | 1    | 5.74      |
| ZkNodeDkgFold                | 108.81  | 3    | 326.44    |
| ZkPkAggregation              | 0.30    | 1    | 0.30      |

Sum of aggregation job tracked time: **383.75 s** (parallel CPU work; not P1/P2 wall clock).

### Folded on-chain artifacts (exported for Π_DKG / Π_dec gas)

| Artifact              | Proof (bytes) | Public inputs (bytes) |
| --------------------- | ------------- | --------------------- |
| dkg_aggregator        | 10944         | 384                   |
| decryption_aggregator | 10944         | 3616                  |

## Raw circuit benchmark JSON (Nargo)

Source files for the **Circuit Benchmarks** table. Persist this directory with
`crisp_verify_gas.json` (and optional `integration_summary.json`) to regenerate the report without
re-running the integration test.

| File                                                  |
| ----------------------------------------------------- |
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
