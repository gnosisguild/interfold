# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-07 09:29:51 UTC

**Git Branch:** `chunk-circuits`  
**Git Commit:** `a84b0bf321ba650489096cbbf3afda7e079fc53f`

**Committee Size:** `H=5`, `N=9`, `T=4`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting                                               | Value          |
| ----------------------------------------------------- | -------------- |
| Benchmark mode                                        | `insecure`     |
| BFV preset (artifacts)                                | `insecure-512` |
| `proof_aggregation_enabled`                           | true           |
| `BENCHMARK_MULTITHREAD_JOBS` (max concurrent ZK jobs) | 13             |
| Rayon worker threads                                  | N/A            |
| CPU cores (host)                                      | N/A            |
| Verbose logging (`run_benchmarks.sh --verbose`)       | true           |

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

> **Incomplete on-chain verify gas:** 2 of 3 artifact verify-gas values are **N/A**. Re-run
> `./run_benchmarks.sh` and ensure `extract_crisp_verify_gas.sh` completes (CRISP test +
> `test_trbfv_actor` + EVM replay). Calldata gas alone is not sufficient for audit sign-off.

> **No integration summary:** Role/phase **wall-clock** rows and multithread job tables require
> `integration_summary.json` or embedded `integration_summary` in `crisp_verify_gas.json`.

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

> **Warning:** `test_trbfv_actor` failed during gas extraction (exit 101). Π_DKG / Π_dec verify gas
> and phase rows below may reflect **Nargo-only** estimates or stale data. Re-run
> `./run_benchmarks.sh` after a successful integration export.

## Protocol Summary

### Circuit Benchmarks (isolated Nargo + Barretenberg)

Single-circuit `bb prove` on the benchmark oracle witness (not the integration actor pipeline).

| Circuit              | Constraints | Prove (s) | Verify (ms) | Proof (KB) |
| -------------------- | ----------- | --------- | ----------- | ---------- |
| C0                   | 6810        | 0.11      | 12.76       | 14.31      |
| C1                   | 0           | 0.00      | 0.00        | 0.00       |
| C2a                  | 117086      | 0.49      | 11.16       | 14.31      |
| C2b                  | 0           | 0.00      | 0.00        | 0.00       |
| C3a                  | 120462      | 0.51      | 12.10       | 14.31      |
| C3b                  | 120462      | 0.51      | 12.10       | 14.31      |
| C4a                  | 79200       | 0.37      | 11.74       | 14.31      |
| C4b                  | 79200       | 0.37      | 11.74       | 14.31      |
| C5                   | 36723       | 0.24      | 10.93       | 14.31      |
| user_data_encryption | 53695       | 0.30      | 11.57       | 14.31      |
| C6                   | 86892       | 0.42      | 12.14       | 14.31      |
| C7                   | 142324      | 0.53      | 11.91       | 14.31      |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
| -------- | ---------- | ----------------- | ---------- | ------------ | --------- |
| Π_DKG    | 14.31 KB   | 0.19 KB           | N/A        | 201352       | N/A       |
| Π_user   | 14.31 KB   | 0.12 KB           | 2982202    | 200628       | 3182830   |
| Π_dec    | 14.31 KB   | 3.44 KB           | N/A        | 215732       | N/A       |

### Role / Phase / Activity

| Role            | Phase | Activity                                  | Metric         | Duration | Proof size | Bandwidth |
| --------------- | ----- | ----------------------------------------- | -------------- | -------- | ---------- | --------- |
| Each ciphernode | P1    | one-time DKG participation (test harness) | —              | N/A      | 85.88 KB   | 87.50 KB  |
| Aggregator      | P2    | C5 + Π_DKG fold (aggregator span)         | —              | N/A      | 14.31 KB   | 14.50 KB  |
| User            | P3    | per user input                            | isolated_nargo | 0.57 s   | 14.31 KB   | 14.44 KB  |
| Each ciphernode | P4    | per computation output (C6)               | isolated_nargo | 0.42 s   | 14.31 KB   | 14.50 KB  |
| Aggregator      | P4    | C7 + Π_dec fold (full publish→aggregate)  | —              | N/A      | 14.31 KB   | 17.75 KB  |

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
