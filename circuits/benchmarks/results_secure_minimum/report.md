# Interfold ZK Circuit Benchmarks

**Generated:** 2026-08-06 18:57:46 UTC

**Git Branch:** `chunk-circuits`  
**Git Commit:** `2031bb8f06c609d305c6f90f10838b92283b2d04`

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

## Comparison with Previous Secure-Minimum Run

The previous run used commit `ee831b99`. Both runs used the secure-8192 preset, the minimum
committee, 13 concurrent multithread jobs, and the same Apple M4 Pro host.

### Circuit Size and Isolated Proving Time

| Circuit | Constraints before | Constraints now | Change | Prove before (s) | Prove now (s) |
|---------|--------------------:|----------------:|-------:|-----------------:|--------------:|
| C1 | 2,223,114 | 2,226,972 | +3,858 | 6.23 | 6.06 |
| C2a | 1,446,311 | 1,463,743 | +17,432 | 4.04 | 4.09 |
| C2b | 2,888,964 | 2,905,880 | +16,916 | 7.50 | 7.53 |
| C3a/C3b | 3,475,203 | 3,478,343 | +3,140 | 9.20 | 9.30 |
| C4a/C4b | 1,746,030 | 1,760,221 | +14,191 | 4.77 | 4.74 |

C0, C5, user-data encryption, C6, and C7 constraint counts did not change. All isolated proof
sizes stayed at `14.31 KB`.

### Recursive Artifact Sizes

| Artifact | Proof before | Proof now | Public inputs before | Public inputs now | Total gas before | Total gas now |
|----------|-------------:|----------:|---------------------:|------------------:|----------------:|--------------:|
| Π_DKG | 10.44 KB | 10.44 KB | 384 B | 448 B | 3,298,854 | 3,304,409 |
| Π_user | 14.31 KB | 14.31 KB | 0.12 KB | 0.12 KB | 3,182,782 | 3,182,782 |
| Π_dec | 10.44 KB | 10.44 KB | 3.56 KB | 3.56 KB | 3,903,520 | 3,903,870 |

### Execution Time

| Measurement | Before (s) | Now (s) | Change (s) |
|-------------|-----------:|--------:|-----------:|
| Each ciphernode DKG participation | 928.83 | 649.32 | -279.51 |
| Aggregator P2 | 128.88 | 129.51 | +0.63 |
| User input proving | 9.15 | 9.07 | -0.08 |
| C6 per-node proving | 8.38 | 8.18 | -0.20 |
| Aggregator P4, full publish to plaintext | 128.98 | 156.93 | +27.95 |
| Aggregator P4, pending to plaintext | 40.68 | 40.14 | -0.54 |
| Entire integration test | 1,066.55 | 814.98 | -251.57 |
| Sum of tracked job wall time | 10,792.65 | 7,084.91 | -3,707.74 |

The tracked-job sum is not end-to-end latency. Jobs run in parallel on the shared Rayon pool.

---

## Fair C2 Comparison from Integration Run

The integration benchmark records `ZkShareComputation` around the complete C2 request path. Each
request is one C2a or C2b operation; the current request includes chunk proving, chunk batching, and
terminal finalization. This is the fair runtime comparison available from the saved benchmark data.

| Work unit | Previous (`ee831b99`) | Current (`2031bb8`) | Change |
|-----------|----------------------:|--------------------:|-------:|
| One C2a or C2b request | 602.91 s | 216.94 s | -385.97 s (-64.0%) |
| Six C2a/C2b requests | 3,617.45 s | 1,301.65 s | -2,315.80 s (-64.0%) |
| C2a/C2b chunk fold, per node | not recorded | 15.15 s | new measurement |

The saved integration summary does not contain separate gate counts, proof sizes, or proving times
for the base, chunk, batch, and terminal circuits. The isolated `C2a` and `C2b` rows above are the
legacy monolithic entrypoints, so they cannot provide a fair per-stage size comparison.

---

## Comparison with Main

The `main` baseline used commit `876ab64e`, Nargo `1.0.0-beta.16`, and Barretenberg
`3.0.0-nightly.20260102`. The current run uses Nargo `1.0.0-beta.26` and Barretenberg `5.1.0`.
The size and timing deltas below are therefore directional, not strict toolchain-controlled
regressions.

### C2 Topology Note

The `C2a` and `C2b` rows below are still generated from the legacy monolithic benchmark
entrypoints, `dkg/sk_share_computation` and `dkg/e_sm_share_computation`. They do not represent
the total cost of the current chunked C2 path and must not be read as a per-track regression.

The current secure path keeps two external C2 tracks, SK (C2a) and ESM (C2b), with one terminal
proof per track. Internally, each track uses one base circuit, 16 coefficient-chunk proofs, four
chunk batches, and one terminal finalizer. A fair isolated comparison needs separate benchmark rows
for those base, chunk, batch, and finalizer circuits; the integration timings and recursive artifact
gas rows below capture the protocol-level result instead.

### Circuit Size

| Circuit | Main constraints | Current constraints | Change |
|---------|-----------------:|--------------------:|-------:|
| C1 | 2,223,151 | 2,226,972 | +3,821 |
| C2a | 1,446,348 | 1,463,743 | +17,395 |
| C2b | 2,889,001 | 2,905,880 | +16,879 |
| C3a/C3b | 3,475,239 | 3,478,343 | +3,104 |
| C4a/C4b | 1,746,067 | 1,760,221 | +14,154 |

### Artifact Size and Gas

| Artifact | Main proof | Current proof | Main public inputs | Current public inputs | Main total gas | Current total gas |
|----------|-----------:|--------------:|-------------------:|---------------------:|---------------:|------------------:|
| Pi_DKG | 10.69 KB | 10.44 KB | 384 B | 448 B | 3,294,634 | 3,304,409 |
| Pi_user | 15.88 KB | 14.31 KB | 0.12 KB | 0.12 KB | 3,166,048 | 3,182,782 |
| Pi_dec | 10.69 KB | 10.44 KB | 3.47 KB | 3.56 KB | 3,828,269 | 3,903,870 |

### Execution Time

| Measurement | Main (s) | Current (s) | Change (s) |
|-------------|---------:|------------:|-----------:|
| Each ciphernode DKG participation | 591.11 | 649.32 | +58.21 |
| Aggregator P2 | 150.01 | 129.51 | -20.50 |
| User input proving | 11.00 | 9.07 | -1.93 |
| C6 per-node proving | 10.26 | 8.18 | -2.08 |
| Aggregator P4, full publish to plaintext | 145.38 | 156.93 | +11.55 |
| Aggregator P4, pending to plaintext | 49.25 | 40.14 | -9.11 |
| Entire integration test | 745.33 | 814.98 | +69.65 |
| Sum of tracked job wall time | 6,336.15 | 7,084.91 | +748.76 |

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
| C0 | 287727 | 1.07 | 10.96 | 14.31 |
| C1 | 2226972 | 6.06 | 10.97 | 14.31 |
| C2a | 1463743 | 4.09 | 11.35 | 14.31 |
| C2b | 2905880 | 7.53 | 11.19 | 14.31 |
| C3a | 3478343 | 9.30 | 11.44 | 14.31 |
| C3b | 3478343 | 9.30 | 11.44 | 14.31 |
| C4a | 1760221 | 4.74 | 11.26 | 14.31 |
| C4b | 1760221 | 4.74 | 11.26 | 14.31 |
| C5 | 754560 | 2.36 | 11.03 | 14.31 |
| user_data_encryption | 1688639 | 4.91 | 11.68 | 14.31 |
| C6 | 2977228 | 8.18 | 11.42 | 14.31 |
| C7 | 108461 | 0.43 | 11.45 | 14.31 |

### Artifacts

| Artifact | Proof size | Public input size | Verify gas | Calldata gas | Total gas |
|----------|------------|-------------------|------------|--------------|-----------|
| Π_DKG | 10.44 KB | 0.44 KB | 3129833 | 174576 | 3304409 |
| Π_user | 14.31 KB | 0.12 KB | 2982202 | 200580 | 3182782 |
| Π_dec | 10.44 KB | 3.56 KB | 3716674 | 187196 | 3903870 |

### Role / Phase / Activity

| Role | Phase | Activity | Metric | Duration | Proof size | Bandwidth |
|------|-------|----------|--------|----------|------------|-----------|
| Each ciphernode | P1 | one-time DKG participation (test harness) | wall_clock | 649.32 s | 114.50 KB | 116.00 KB |
| Aggregator | P2 | C5 + Π_DKG fold (aggregator span) | wall_clock | 129.51 s | 10.44 KB | 10.88 KB |
| User | P3 | per user input | isolated_nargo | 9.07 s | 14.31 KB | 14.44 KB |
| Each ciphernode | P4 | per computation output (C6) | isolated_nargo | 8.18 s | 14.31 KB | 14.50 KB |
| Aggregator | P4 | C7 + Π_dec fold (full publish→aggregate) | wall_clock | 156.93 s | 10.44 KB | 14.00 KB |
| Aggregator | P4 | C7 + fold only (pending→plaintext span) | wall_clock | 40.14 s | 10.44 KB | 14.00 KB |

_P2 **tracked_job_wall** sum (ZkDkgAggregation + ZkPkAggregation, parallelizable): **18.52 s** — not comparable to P2 wall_clock row above._

## Integration test (`test_trbfv_actor`)

### End-to-end phase timings (integration test)

| Phase | Metric | Duration (s) |
|-------|--------|---------------|
| Starting trbfv actor test | `wall_clock` | 0.00 |
| Setup completed | `wall_clock` | 0.90 |
| Committee Setup Completed | `wall_clock` | 7.03 |
| Committee Finalization Complete | `wall_clock` | 0.00 |
| Aggregator P2: PkAggregation pending -> PublicKeyAggregated (wall) | `wall_clock` | 129.51 |
| ThresholdShares -> PublicKeyAggregated | `wall_clock` | 649.32 |
| E3Request -> PublicKeyAggregated | `wall_clock` | 649.83 |
| Application CT Gen | `wall_clock` | 0.29 |
| Running FHE Application | `wall_clock` | 0.00 |
| Aggregator P4: Aggregation pending -> PlaintextAggregated (wall) | `wall_clock` | 40.14 |
| Ciphertext published -> PlaintextAggregated | `wall_clock` | 156.93 |
| Entire Test | `wall_clock` | 814.98 |

### Multithread job timings (`tracked_job_wall`)

| Name | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| CalculateDecryptionKey | 0.06 | 3 | 0.18 |
| CalculateDecryptionShare | 0.17 | 3 | 0.50 |
| CalculateThresholdDecryption | 0.15 | 1 | 0.15 |
| GenEsiSss | 0.06 | 3 | 0.19 |
| GenPkShareAndSkSss | 0.11 | 3 | 0.33 |
| NodeDkgFold/c2ab_chunk_fold | 15.15 | 3 | 45.45 |
| NodeDkgFold/c3a_fold | 88.25 | 3 | 264.76 |
| NodeDkgFold/c3ab_fold | 6.89 | 3 | 20.67 |
| NodeDkgFold/c3b_fold | 87.92 | 3 | 263.77 |
| NodeDkgFold/c4ab_fold | 7.28 | 3 | 21.83 |
| NodeDkgFold/node_fold | 16.76 | 3 | 50.29 |
| ZkDecryptedSharesAggregation | 2.67 | 1 | 2.67 |
| ZkDecryptionAggregation | 37.37 | 1 | 37.37 |
| ZkDkgAggregation | 3.98 | 1 | 3.98 |
| ZkDkgShareDecryption | 19.58 | 6 | 117.46 |
| ZkNodeDkgFold | 125.53 | 3 | 376.59 |
| ZkNodesFoldStep | 4.18 | 2 | 8.36 |
| ZkPkAggregation | 14.53 | 1 | 14.53 |
| ZkPkBfv | 2.60 | 3 | 7.80 |
| ZkPkGeneration | 84.87 | 3 | 254.62 |
| ZkShareComputation | 216.94 | 6 | 1301.65 |
| ZkShareEncryption | 112.11 | 36 | 4035.97 |
| ZkThresholdShareDecryption | 84.95 | 3 | 254.84 |
| ZkVerifyShareDecryptionProofs | 0.05 | 3 | 0.14 |
| ZkVerifyShareProofs | 0.16 | 5 | 0.81 |

Sum of tracked job wall time: **7084.91 s** — **not** end-to-end latency (jobs run in parallel up to `BENCHMARK_MULTITHREAD_JOBS`).

### NodeDkgFold sub-steps (`tracked_job_wall`, per fold prove)

| Step | Avg (s) | Runs | Total (s) |
|------|---------|------|-----------|
| c2ab_chunk_fold | 15.15 | 3 | 45.45 |
| c3a_fold | 88.25 | 3 | 264.76 |
| c3ab_fold | 6.89 | 3 | 20.67 |
| c3b_fold | 87.92 | 3 | 263.77 |
| c4ab_fold | 7.28 | 3 | 21.83 |
| node_fold | 16.76 | 3 | 50.29 |

### Aggregation jobs (`tracked_job_wall`)

| Operation | Avg (s) | Runs | Total (s) |
|-----------|---------|------|-----------|
| ZkDecryptedSharesAggregation | 2.67 | 1 | 2.67 |
| ZkDecryptionAggregation | 37.37 | 1 | 37.37 |
| ZkDkgAggregation | 3.98 | 1 | 3.98 |
| ZkNodeDkgFold | 125.53 | 3 | 376.59 |
| ZkPkAggregation | 14.53 | 1 | 14.53 |

Sum of aggregation job tracked time: **435.15 s** (parallel CPU work; not P1/P2 wall clock).

### Folded on-chain artifacts (exported for Π_DKG / Π_dec gas)

| Artifact | Proof (bytes) | Public inputs (bytes) |
|----------|---------------|------------------------|
| dkg_aggregator | 10688 | 448 |
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
