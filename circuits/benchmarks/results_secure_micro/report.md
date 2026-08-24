# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-24 18:01:54 UTC

**Git Branch:** `main`  
**Git Commit:** `de8f1a8bddbed0d6562a9ccadac4e1fa15e1fcc2`

**Committee Size:** `H=5`, `N=9`, `T=4`

## Run configuration

Settings for this benchmark run (integration test + Nargo circuit benches on the same host).

### Integration test (`test_trbfv_actor`)

| Setting | Value |
|---------|-------|
| Benchmark mode | `secure` |
| BFV preset (artifacts) | `secure-8192` |
| BFV preset (enum) | `SecureThreshold8192` |
| λ (smudging / error) | 46 |
| Nodes spawned (builder) | 16 |
| Network model | `in_process_bus` |
| Testmode harness | true |
| `proof_aggregation_enabled` | true |
| `BENCHMARK_MULTITHREAD_JOBS` (max concurrent ZK jobs) | 13 |
| Rayon worker threads | 11 |
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
| C0 | 287727 | 1.11 | 12.38 | 14.31 |
| C1 | 2223201 | 6.31 | 10.95 | 14.31 |
| C2a | 4283789 | 11.58 | 12.71 | 14.31 |
| C2b | 5726442 | 15.15 | 11.59 | 14.31 |
| C3a | 3563483 | 9.99 | 11.92 | 14.31 |
| C3b | 3563483 | 9.99 | 11.92 | 14.31 |
| C4a | 2418273 | 6.79 | 11.69 | 14.31 |
| C4b | 2418273 | 6.79 | 11.69 | 14.31 |
| C5 | 1426371 | 4.59 | 11.44 | 14.31 |
| user_data_encryption | 1685910 | 5.11 | 12.37 | 14.31 |
| C6 | 3001812 | 8.24 | 11.24 | 14.31 |
| C7 | 191201 | 0.70 | 12.88 | 14.31 |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
|----------|------------|-------------------|------------|--------------|-----------|
| Π_DKG | 10.44 KiB | 0.66 KiB | 3143741 | 177068 | 3320809 |
| Π_user | 14.31 KiB | 0.12 KiB | 3033986 | 200484 | 3234470 |
| Π_dec | 10.44 KiB | 3.84 KiB | 3761767 | 190448 | 3952215 |

### Role / Phase / Activity

| Role | Phase | Activity | Metric | Duration | Proof size | Bandwidth |
|------|-------|----------|--------|----------|------------|-----------|
| Each ciphernode | P1 | one-time DKG participation (test harness) | wall_clock | 5067.66 s | 114.50 KiB | 117.56 KiB |
| Aggregator | P2 | C5 + Π_DKG fold (aggregator span) | wall_clock | 548.46 s | 10.44 KiB | 11.09 KiB |
| User | P3 | per user input | isolated_nargo | 9.29 s | 14.31 KiB | 14.44 KiB |
| Each ciphernode | P4 | per computation output (C6) | isolated_nargo | 8.24 s | 14.31 KiB | 14.50 KiB |
| Aggregator | P4 | C7 + Π_dec fold (full publish→aggregate) | wall_clock | 450.70 s | 10.44 KiB | 14.28 KiB |
| Aggregator | P4 | C7 + fold only (pending→plaintext span) | wall_clock | 226.60 s | 10.44 KiB | 14.28 KiB |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **423.27 s** — not comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase | Metric | Duration (s) |
|-------|--------|---------------|
| Starting trbfv actor test | `wall_clock` | 0.00 |
| Setup completed | `wall_clock` | 1.82 |
| Committee Setup Completed | `wall_clock` | 16.06 |
| Committee Finalization Complete | `wall_clock` | 0.00 |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 548.46 |
| ThresholdShares -> PublicKeyAggregated | `wall_clock` | 5067.66 |
| E3Request -> PublicKeyAggregated | `wall_clock` | 5068.17 |
| Application CT Gen | `wall_clock` | 1.81 |
| Running FHE Application | `wall_clock` | 0.00 |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall) | `wall_clock` | 226.60 |
| Ciphertext published -> PlaintextAggregated | `wall_clock` | 450.70 |
| Entire Test | `wall_clock` | 5538.56 |

### Multithread job timings (`tracked_job_wall`)

| Name | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| CalculateDecryptionKey | 0.04 | 9 | 0.38 |
| CalculateDecryptionShare | 0.36 | 9 | 3.26 |
| CalculateThresholdDecryption | 85.82 | 8 | 686.58 |
| GenEsiSss | 20.42 | 9 | 183.74 |
| GenPkShareAndSkSss | 0.45 | 9 | 4.07 |
| NodeDkgFold/c2ab_fold | 20.55 | 9 | 184.95 |
| NodeDkgFold/c3a_fold | 518.86 | 9 | 4669.78 |
| NodeDkgFold/c3ab_fold | 13.13 | 9 | 118.21 |
| NodeDkgFold/c3b_fold | 503.95 | 9 | 4535.57 |
| NodeDkgFold/c4ab_fold | 12.90 | 9 | 116.13 |
| NodeDkgFold/node_fold | 25.17 | 9 | 226.54 |
| ZkDecryptedSharesAggregation | 7.19 | 5 | 35.93 |
| ZkDecryptionAggregation | 220.16 | 5 | 1100.79 |
| ZkDkgAggregation | 20.97 | 8 | 167.75 |
| ZkDkgShareDecryption | 52.42 | 18 | 943.60 |
| ZkNodeDkgFold | 862.13 | 9 | 7759.15 |
| ZkNodesFoldStep | 20.12 | 40 | 804.94 |
| ZkPkAggregation | 31.94 | 8 | 255.51 |
| ZkPkBfv | 6.97 | 9 | 62.70 |
| ZkPkGeneration | 96.18 | 9 | 865.64 |
| ZkShareComputation | 52.70 | 18 | 948.55 |
| ZkShareEncryption | 100.10 | 432 | 43241.08 |
| ZkThresholdShareDecryption | 202.94 | 9 | 1826.46 |
| ZkVerifyShareDecryptionProofs | 0.15 | 9 | 1.32 |
| ZkVerifyShareProofs | 0.49 | 25 | 12.17 |

Sum of tracked job wall time: **68754.81 s** — **not** end-to-end latency (jobs run in parallel up to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| c2ab_fold | 20.55 | 9 | 184.95 |
| c3a_fold | 518.86 | 9 | 4669.78 |
| c3ab_fold | 13.13 | 9 | 118.21 |
| c3b_fold | 503.95 | 9 | 4535.57 |
| c4ab_fold | 12.90 | 9 | 116.13 |
| node_fold | 25.17 | 9 | 226.54 |

### Aggregation jobs (`tracked_job_wall`)

| Operation | Avg (s) | Runs | Total (s) |
|-----------|---------|------|-----------|
| ZkDecryptedSharesAggregation | 7.19 | 5 | 35.93 |
| ZkDecryptionAggregation | 220.16 | 5 | 1100.79 |
| ZkDkgAggregation | 20.97 | 8 | 167.75 |
| ZkNodeDkgFold | 862.13 | 9 | 7759.15 |
| ZkPkAggregation | 31.94 | 8 | 255.51 |

Sum of aggregation job tracked time: **9319.14 s** (parallel CPU work; not P1/P2 wall clock).

### Folded on-chain artifacts (exported for Π_DKG / Π_dec gas)

| Artifact | Proof (bytes) | Public inputs (bytes) |
|----------|---------------|------------------------|
| dkg_aggregator | 10688 | 672 |
| decryption_aggregator | 10688 | 3936 |

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
