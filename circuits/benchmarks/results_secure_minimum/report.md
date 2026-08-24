# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-23 17:44:22 UTC

**Git Branch:** `main`  
**Git Commit:** `de8f1a8bddbed0d6562a9ccadac4e1fa15e1fcc2`

**Committee Size:** `H=2`, `N=3`, `T=1`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting | Value |
|---------|-------|
| Benchmark mode | `secure` |
| BFV preset (artifacts) | `secure-8192` |
| BFV preset (enum) | `SecureThreshold8192` |
| λ (smudging / error) | 46 |
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

| Circuit | Constraints | Prove (s) | Verify (ms) | Proof (KiB) |
|---------|-------------|-----------|-------------|------------|
| C0 | 287727 | 1.03 | 11.47 | 14.31 |
| C1 | 2223201 | 6.21 | 11.35 | 14.31 |
| C2a | 1446311 | 4.04 | 11.29 | 14.31 |
| C2b | 2888964 | 7.46 | 11.27 | 14.31 |
| C3a | 3563483 | 9.43 | 11.27 | 14.31 |
| C3b | 3563483 | 9.43 | 11.27 | 14.31 |
| C4a | 1746030 | 4.68 | 11.36 | 14.31 |
| C4b | 1746030 | 4.68 | 11.36 | 14.31 |
| C5 | 754560 | 2.45 | 11.54 | 14.31 |
| user_data_encryption | 1685910 | 4.94 | 11.56 | 14.31 |
| C6 | 3001812 | 8.25 | 11.63 | 14.31 |
| C7 | 108461 | 0.42 | 11.52 | 14.31 |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
|----------|------------|-------------------|------------|--------------|-----------|
| Π_DKG | 10.44 KiB | 0.38 KiB | 3125072 | 173444 | 3298516 |
| Π_user | 14.31 KiB | 0.12 KiB | 3034250 | 200580 | 3234830 |
| Π_dec | 10.44 KiB | 3.56 KiB | 3716747 | 187136 | 3903883 |

### Role / Phase / Activity

| Role | Phase | Activity | Metric | Duration | Proof size | Bandwidth |
|------|-------|----------|--------|----------|------------|-----------|
| Each ciphernode | P1 | one-time DKG participation (test harness) | wall_clock | 526.21 s | 114.50 KiB | 115.88 KiB |
| Aggregator | P2 | C5 + Π_DKG fold (aggregator span) | wall_clock | 137.87 s | 10.44 KiB | 10.81 KiB |
| User | P3 | per user input | isolated_nargo | 9.14 s | 14.31 KiB | 14.44 KiB |
| Each ciphernode | P4 | per computation output (C6) | isolated_nargo | 8.25 s | 14.31 KiB | 14.50 KiB |
| Aggregator | P4 | C7 + Π_dec fold (full publish→aggregate) | wall_clock | 126.81 s | 10.44 KiB | 14.00 KiB |
| Aggregator | P4 | C7 + fold only (pending→plaintext span) | wall_clock | 39.83 s | 10.44 KiB | 14.00 KiB |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **18.78 s** — not comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase | Metric | Duration (s) |
|-------|--------|---------------|
| Starting trbfv actor test | `wall_clock` | 0.00 |
| Setup completed | `wall_clock` | 0.86 |
| Committee Setup Completed | `wall_clock` | 7.02 |
| Committee Finalization Complete | `wall_clock` | 0.01 |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 137.87 |
| ThresholdShares -> PublicKeyAggregated | `wall_clock` | 526.21 |
| E3Request -> PublicKeyAggregated | `wall_clock` | 526.72 |
| Application CT Gen | `wall_clock` | 0.29 |
| Running FHE Application | `wall_clock` | 0.00 |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall) | `wall_clock` | 39.83 |
| Ciphertext published -> PlaintextAggregated | `wall_clock` | 126.81 |
| Entire Test | `wall_clock` | 661.70 |

### Multithread job timings (`tracked_job_wall`)

| Name | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| CalculateDecryptionKey | 0.04 | 3 | 0.11 |
| CalculateDecryptionShare | 0.16 | 3 | 0.49 |
| CalculateThresholdDecryption | 0.15 | 1 | 0.15 |
| GenEsiSss | 0.08 | 3 | 0.24 |
| GenPkShareAndSkSss | 0.16 | 3 | 0.49 |
| NodeDkgFold/c2ab_fold | 15.81 | 3 | 47.42 |
| NodeDkgFold/c3a_fold | 90.05 | 3 | 270.15 |
| NodeDkgFold/c3ab_fold | 4.57 | 3 | 13.71 |
| NodeDkgFold/c3b_fold | 89.86 | 3 | 269.58 |
| NodeDkgFold/c4ab_fold | 4.64 | 3 | 13.92 |
| NodeDkgFold/node_fold | 10.87 | 3 | 32.60 |
| ZkDecryptedSharesAggregation | 2.54 | 1 | 2.54 |
| ZkDecryptionAggregation | 37.28 | 1 | 37.28 |
| ZkDkgAggregation | 3.93 | 1 | 3.93 |
| ZkDkgShareDecryption | 22.13 | 6 | 132.81 |
| ZkNodeDkgFold | 123.77 | 3 | 371.31 |
| ZkNodesFoldStep | 4.15 | 2 | 8.29 |
| ZkPkAggregation | 14.85 | 1 | 14.85 |
| ZkPkBfv | 2.62 | 3 | 7.85 |
| ZkPkGeneration | 44.16 | 3 | 132.49 |
| ZkShareComputation | 17.83 | 6 | 106.97 |
| ZkShareEncryption | 103.76 | 36 | 3735.33 |
| ZkThresholdShareDecryption | 86.46 | 3 | 259.37 |
| ZkVerifyShareDecryptionProofs | 0.04 | 3 | 0.12 |
| ZkVerifyShareProofs | 0.14 | 5 | 0.69 |

Sum of tracked job wall time: **5462.66 s** — **not** end-to-end latency (jobs run in parallel up to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| c2ab_fold | 15.81 | 3 | 47.42 |
| c3a_fold | 90.05 | 3 | 270.15 |
| c3ab_fold | 4.57 | 3 | 13.71 |
| c3b_fold | 89.86 | 3 | 269.58 |
| c4ab_fold | 4.64 | 3 | 13.92 |
| node_fold | 10.87 | 3 | 32.60 |

### Aggregation jobs (`tracked_job_wall`)

| Operation | Avg (s) | Runs | Total (s) |
|-----------|---------|------|-----------|
| ZkDecryptedSharesAggregation | 2.54 | 1 | 2.54 |
| ZkDecryptionAggregation | 37.28 | 1 | 37.28 |
| ZkDkgAggregation | 3.93 | 1 | 3.93 |
| ZkNodeDkgFold | 123.77 | 3 | 371.31 |
| ZkPkAggregation | 14.85 | 1 | 14.85 |

Sum of aggregation job tracked time: **429.91 s** (parallel CPU work; not P1/P2 wall clock).

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
