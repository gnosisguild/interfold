# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-05 12:54:19 UTC

**Git Branch:** `nargo22`  
**Git Commit:** `3e01de7ca0e87d1d4fe57f41fc1e775af17647ee`

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
| C0 | 6810 | 0.11 | 10.66 | 14.31 |
| C1 | 53448 | 0.29 | 10.90 | 14.31 |
| C2a | 41207 | 0.23 | 10.77 | 14.31 |
| C2b | 79554 | 0.36 | 11.12 | 14.31 |
| C3a | 120078 | 0.49 | 10.88 | 14.31 |
| C3b | 120078 | 0.49 | 10.88 | 14.31 |
| C4a | 62713 | 0.31 | 10.89 | 14.31 |
| C4b | 62713 | 0.31 | 10.89 | 14.31 |
| C5 | 21464 | 0.16 | 10.65 | 14.31 |
| user_data_encryption | 53695 | 0.28 | 10.35 | 14.31 |
| C6 | 86892 | 0.40 | 10.67 | 14.31 |
| C7 | 89602 | 0.37 | 10.97 | 14.31 |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
|----------|------------|-------------------|------------|--------------|-----------|
| Π_DKG | 10.44 KB | 0.38 KB | 3125181 | 173552 | 3298733 |
| Π_user | 14.31 KB | 0.12 KB | 2982298 | 200568 | 3182866 |
| Π_dec | 10.44 KB | 3.56 KB | 3692449 | 187016 | 3879465 |

### Role / Phase / Activity

| Role | Phase | Activity | Metric | Duration | Proof size | Bandwidth |
|------|-------|----------|--------|----------|------------|-----------|
| Each ciphernode | P1 | one-time DKG participation (test harness) | wall_clock | 121.77 s | 114.50 KB | 115.56 KB |
| Aggregator | P2 | C5 + Π_DKG fold (aggregator span) | wall_clock | 110.72 s | 10.44 KB | 10.81 KB |
| User | P3 | per user input | isolated_nargo | 0.53 s | 14.31 KB | 14.44 KB |
| Each ciphernode | P4 | per computation output (C6) | isolated_nargo | 0.40 s | 14.31 KB | 14.50 KB |
| Aggregator | P4 | C7 + Π_dec fold (full publish→aggregate) | wall_clock | 42.70 s | 10.44 KB | 14.00 KB |
| Aggregator | P4 | C7 + fold only (pending→plaintext span) | wall_clock | 39.60 s | 10.44 KB | 14.00 KB |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **4.52 s** — not comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase | Metric | Duration (s) |
|-------|--------|---------------|
| Starting trbfv actor test | `wall_clock` | 0.00 |
| Setup completed | `wall_clock` | 0.85 |
| Committee Setup Completed | `wall_clock` | 7.03 |
| Committee Finalization Complete | `wall_clock` | 0.01 |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 110.72 |
| ThresholdShares -> PublicKeyAggregated | `wall_clock` | 121.77 |
| E3Request -> PublicKeyAggregated | `wall_clock` | 122.28 |
| Application CT Gen | `wall_clock` | 0.01 |
| Running FHE Application | `wall_clock` | 0.00 |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall) | `wall_clock` | 39.60 |
| Ciphertext published -> PlaintextAggregated | `wall_clock` | 42.70 |
| Entire Test | `wall_clock` | 172.87 |

### Multithread job timings (`tracked_job_wall`)

| Name | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| CalculateDecryptionKey | 0.01 | 3 | 0.02 |
| CalculateDecryptionShare | 0.02 | 3 | 0.07 |
| CalculateThresholdDecryption | 0.02 | 1 | 0.02 |
| GenEsiSss | 1.18 | 3 | 3.55 |
| GenPkShareAndSkSss | 0.01 | 3 | 0.03 |
| NodeDkgFold/c2ab_fold | 17.58 | 3 | 52.73 |
| NodeDkgFold/c3a_fold | 66.82 | 3 | 200.45 |
| NodeDkgFold/c3ab_fold | 7.17 | 3 | 21.51 |
| NodeDkgFold/c3b_fold | 66.76 | 3 | 200.28 |
| NodeDkgFold/c4ab_fold | 7.31 | 3 | 21.92 |
| NodeDkgFold/node_fold | 17.52 | 3 | 52.56 |
| ZkDecryptedSharesAggregation | 1.50 | 1 | 1.50 |
| ZkDecryptionAggregation | 38.08 | 1 | 38.08 |
| ZkDkgAggregation | 4.05 | 1 | 4.05 |
| ZkDkgShareDecryption | 1.12 | 6 | 6.74 |
| ZkNodeDkgFold | 98.98 | 3 | 296.94 |
| ZkNodesFoldStep | 5.01 | 2 | 10.02 |
| ZkPkAggregation | 0.47 | 1 | 0.47 |
| ZkPkBfv | 0.21 | 3 | 0.63 |
| ZkPkGeneration | 2.98 | 3 | 8.94 |
| ZkShareComputation | 1.87 | 6 | 11.20 |
| ZkShareEncryption | 3.36 | 24 | 80.74 |
| ZkThresholdShareDecryption | 2.91 | 3 | 8.73 |
| ZkVerifyShareDecryptionProofs | 0.05 | 3 | 0.15 |
| ZkVerifyShareProofs | 0.11 | 5 | 0.54 |

Sum of tracked job wall time: **1021.87 s** — **not** end-to-end latency (jobs run in parallel up to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| c2ab_fold | 17.58 | 3 | 52.73 |
| c3a_fold | 66.82 | 3 | 200.45 |
| c3ab_fold | 7.17 | 3 | 21.51 |
| c3b_fold | 66.76 | 3 | 200.28 |
| c4ab_fold | 7.31 | 3 | 21.92 |
| node_fold | 17.52 | 3 | 52.56 |

### Aggregation jobs (`tracked_job_wall`)

| Operation | Avg (s) | Runs | Total (s) |
|-----------|---------|------|-----------|
| ZkDecryptedSharesAggregation | 1.50 | 1 | 1.50 |
| ZkDecryptionAggregation | 38.08 | 1 | 38.08 |
| ZkDkgAggregation | 4.05 | 1 | 4.05 |
| ZkNodeDkgFold | 98.98 | 3 | 296.94 |
| ZkPkAggregation | 0.47 | 1 | 0.47 |

Sum of aggregation job tracked time: **341.05 s** (parallel CPU work; not P1/P2 wall clock).

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
