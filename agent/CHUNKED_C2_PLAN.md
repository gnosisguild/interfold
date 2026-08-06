# Chunked C2 Implementation Plan

Status: design plan. No implementation is included in this document.

## Scope

Reduce the circuit size of C2a and C2b for the current Nargo `1.0.0-beta.26` and Barretenberg
`5.1.0` compatibility unit. Preserve the current DKG protocol, C2 public interface, C2 to C3/C4
commitment links, signed proof multiplicity, and recursive `NodeFold` pipeline.

## Current Baseline

The current C2 circuits are monolithic:

- `circuits/lib/src/core/dkg/share_computation.nr`
- `circuits/bin/dkg/sk_share_computation/src/main.nr`
- `circuits/bin/dkg/e_sm_share_computation/src/main.nr`

Each C2 circuit checks the secret commitment, secret consistency, share range, Reed-Solomon parity,
and share commitments for all coefficients.

The current C2 public interface is:

```text
expected_secret_commitment
|| N_PARTIES * L share commitments
```

The current recursive path consumes this interface through:

- `circuits/bin/recursive_aggregation/c2ab_fold/`
- `circuits/bin/recursive_aggregation/node_fold/`
- `crates/zk-prover/src/circuits/aggregation/node_dkg_fold.rs`

The secure-minimum benchmark reports:

| Circuit | Constraints | Prove time |
| --- | ---: | ---: |
| C2a | 1,446,348 | 5.16 s |
| C2b | 2,889,001 | 9.65 s |

### Current Compatibility Baseline

The following measurements were collected on 2026-08-05 with the current compatibility unit:

- Nargo `1.0.0-beta.26`.
- Barretenberg `5.1.0`.
- Preset `secure-8192`.
- Committee `minimum` (`N=3`, `T=1`, `H=2`).
- Default benchmark oracle.
- Apple M4 Pro, 14 cores.

Each circuit compiled, executed, proved, and verified successfully.

| Circuit | ACIR opcodes | Constraints | Prove time | Proof size |
| --- | ---: | ---: | ---: | ---: |
| C2a | 426,360 | 1,446,311 | 6.181560 s | 14,656 bytes |
| C2b | 827,414 | 2,888,964 | 11.829518 s | 14,656 bytes |

The benchmark JSON files are:

- `circuits/benchmarks/results_secure_minimum/raw/dkg_sk_share_computation_default.json`
- `circuits/benchmarks/results_secure_minimum/raw/dkg_e_sm_share_computation_default.json`

The older values in the table above were generated with Nargo beta.16 and Barretenberg 3.0.0. Do
not use them as the comparison target for the chunked implementation. Use the current compatibility
baseline in this section.

### Current Public-Signal Layouts

The public-signal counts are derived from the current Noir circuit formulas:

| Preset and committee | C2a/C2b | C2abFold | NodeFold |
| --- | ---: | ---: | ---: |
| insecure-512 / minimum (`N=3`, `H=2`, `L=1`) | 4 | 11 | 24 |
| secure-8192 / minimum (`N=3`, `H=2`, `L=3`) | 10 | 23 | 44 |

The formulas are:

```text
C2_PUBLIC_LEN = 1 + N_PARTIES * L_THRESHOLD
C2AB_FOLD_PUBLIC_LEN = 3 + 2 * C2_PUBLIC_LEN
NODE_FOLD_PUBLIC_LEN = 11 + N_PARTIES + 2 * (N_PARTIES + H) * L_THRESHOLD
```

The current secure C2 benchmark public-input files contain 10 fields, or 320 bytes. These counts
are compatibility targets for the final chunk accumulator proof.

## Upstream Design Review

The reference branch is `origin/circuits/split-c2`, based on commit `38d2f1e5`.

It splits each C2 branch into two parts:

1. A base circuit checks the C1 secret commitment and secret consistency. It outputs chunk
   commitments and per-party share commitments.
2. A chunk circuit checks one coefficient range, its share range, and its parity constraints.
3. Recursive aggregation is expected to combine the chunk proofs.

The secure preset uses `CHUNK_SIZE = 512` and `N_CHUNKS = 16` for `N = 8192`.

The branch is not a complete implementation. It does not update Rust witness generation, proof
requests, signed proof handling, peer verification, `NodeFold`, or the current commitment links.
Its `share_computation_chunk` package also contains only `CHUNK_IDX = 0`, so it cannot produce all
secure-preset chunks without additional circuit generation and artifact management.

The upstream party commitment code also uses an older commitment layout. It is not compatible with
the current C2 to C3 and C2 to C4 bindings without a wider protocol change.

## Target Design

### 1. Preserve the external C2 contract

Keep one final C2a proof and one final C2b proof in `ThresholdShareCreated`. Keep the current public
output shape so the existing C2ab, C3, C4, and NodeFold interfaces remain stable where possible.

Keep the current external proof types:

- `ProofType::C2aSkShareComputation`
- `ProofType::C2bESmShareComputation`

The final proof must remain valid for the circuit expected by peer verification and by `c2ab_fold`.
Local base and chunk proofs must not be broadcast as additional C2 proofs.

### 2. Add a base circuit for each C2 branch

The base circuit must preserve the current semantics for:

- C1 to C2 secret commitment binding.
- SK reverse-order normalization.
- ESM reverse-and-center normalization.
- Secret consistency at the Shamir point zero.
- Current per-party and per-modulus share commitments used by C3 and C4.

The base circuit also produces an ordered chunk commitment for every coefficient range. The chunk
commitment must include a domain separator and the chunk index.

The base circuit will still receive the full witness. The first prototype must measure whether the
removed range and parity checks provide the required reduction. If the base circuit remains too
large, a separate design is needed for the party commitments because C3 and C4 currently require
commitments over the complete share polynomial.

### 3. Add a shared chunk circuit

The chunk circuit receives one bounded coefficient range and checks:

- The chunk commitment.
- Share range bounds for all non-secret party slots.
- Reed-Solomon parity for every coefficient and modulus in the chunk.

Use a public chunk index and bind it to the commitment. Do not use the upstream compile-time
`CHUNK_IDX` design as the only index mechanism. A public index avoids one circuit package and one VK
per chunk.

Require `N % CHUNK_SIZE == 0` in the circuit configuration. Do not add implicit padding until a
separate soundness and commitment design exists for a partial final chunk.

### 4. Add a native recursive C2 accumulator

Follow the current sequential accumulator pattern used by `c3_fold`, `c6_fold`, and `nodes_fold`:

- Add a C2 fold circuit.
- Add a C2 fold kernel for the first step.
- Verify the base proof and the first chunk in the genesis step.
- Verify the previous accumulator and one new chunk in every later step.
- Carry the base commitments through the accumulator.
- Enforce that every chunk index is in range and appears exactly once.
- Emit the current C2 public interface from the final accumulator.

The accumulator must reject:

- Missing chunks.
- Duplicate chunks.
- Reordered chunks that do not match their committed index.
- A chunk from another base proof.
- A chunk with a changed public commitment.

The current `c2ab_fold` verifies ZK C2 proofs. The implementation must explicitly resolve the proof
variant boundary before integration. The final C2 accumulator must either produce the ZK proof type
expected by `c2ab_fold` or use a terminal wrapper that converts the accumulator into the required
final C2 proof interface.

## Implementation Phases

### Phase 0: Lock the baseline

1. Record current C2a and C2b constraint, proving-time, and proof-size measurements.
2. Record current public-signal field counts for C2a, C2b, C2ab, and NodeFold.
3. Add or confirm tests for current C2 commitment orientation and public-input layouts.

### Phase 1: Implement and benchmark Noir circuits

1. Add base and chunk modules beside `share_computation.nr`.
2. Add base entry points for SK and ESM.
3. Add a shared chunk entry point.
4. Add chunk-size and chunk-count configuration for every supported preset.
5. Add chunk domain separation and index binding.
6. Compile with `pnpm build:circuits` for insecure and secure presets.
7. Benchmark base, chunk, and accumulator circuits for all supported committee sizes.

Prototype status:

- The SK base, ESM base, and shared chunk packages compile with `nargo check --workspace` and
  `nargo compile` for the active `insecure-512 / minimum` configuration.
- The base packages preserve the C1 commitment checks and the existing C2 party-share commitment
  orientation. They also output ordered chunk commitments.
- The chunk package checks the public chunk index, the chunk commitment, share bounds, and the
  Reed-Solomon parity equations for one coefficient range.
- The prototype base public layout is `1 + N_CHUNKS + N_PARTIES * L_THRESHOLD` fields. The chunk
  public layout is `[chunk_commitment, chunk_idx]`.
- The packages also compile for `secure-8192 / minimum`. The reported ACIR opcode counts are:
  - SK base: `188792`.
  - ESM base: `589846`.
  - Shared chunk: `31238`.
- The `insecure-512 / minimum` ACIR opcode counts are `7207`, `22742`, and `20822` for the same
  packages.
- Proof timings, constraint counts, and the recursive accumulator are still pending. Do not
  integrate the Rust proof path until those measurements show the required reduction.

Do not continue to Rust integration if the base circuit does not provide a meaningful size
reduction. The result must be measured, not inferred from the removal of loops.

### Phase 2: Implement the recursive accumulator

1. Add `c2_fold` and `c2_fold_kernel` under
   `circuits/bin/recursive_aggregation/`.
2. Define fixed public layouts for each active preset and committee size.
3. Implement the genesis and subsequent accumulator transitions.
4. Prove a complete C2a chain and a complete C2b chain.
5. Feed both final proofs into the existing `c2ab_fold`.
6. Prove the complete `C2abFold -> NodeFold` path.

### Phase 3: Update Rust witness and proof generation

Update:

- `crates/zk-helpers/src/circuits/dkg/share_computation/`
- `crates/zk-prover/src/circuits/dkg/share_computation.rs`
- `crates/zk-prover/src/circuits/aggregation/`
- `crates/zk-prover/src/circuits/aggregation/node_dkg_fold.rs`

Required behavior:

- Build one base witness per C2 branch.
- Split `y` into deterministic coefficient chunks.
- Generate all chunk proofs.
- Run the C2 recursive accumulator.
- Return one final C2 proof per branch.
- Preserve `WitnessStack::serialize()` for every generated witness.

### Phase 4: Update protocol and verification boundaries

Review and update as required:

- `crates/events/src/interfold_event/proof.rs`
- `crates/events/src/interfold_event/signed_proof.rs`
- `crates/events/src/interfold_event/compute_request/zk.rs`
- `crates/zk-prover/src/proof_request/`
- `crates/zk-prover/src/share_verification/`
- `crates/zk-prover/src/commitment_links/`

Keep the external C2 proof count unchanged:

```text
C2a x 1, C2b x 1, C3a x L, C3b x L
```

If new `CircuitName` variants are required for local base, chunk, or accumulator proofs, define
their serialization and artifact compatibility explicitly. Do not silently change durable or wire
schemas.

### Phase 5: Build and publish artifacts

1. Add all new circuit packages to `scripts/build-circuits.ts` discovery and artifact copying.
2. Generate EVM, recursive, and recursive-no-ZK VKs as required by each circuit.
3. Regenerate checksums and circuit archives.
4. Bump `required_circuits_version` in the same compatibility change.
5. Regenerate dependent verifier artifacts when a public layout or VK changes.
6. Run the generated-file consistency checks. Do not hand-edit generated outputs.

### Phase 6: Update documentation

Update the same change set:

- `agent/flow-trace/04_DKG_AND_COMPUTATION.md`
- `agent/INVARIANTS.md`
- `agent/CRATES_ARCHITECTURE.md`
- C2 circuit documentation and benchmark reports

Document the final proof multiplicity, chunk coverage rule, public-input layout, proof variant, and
restart or retry behavior for pending chunk proofs.

## Required Invariants

- C2a and C2b remain bound to the matching C1 commitments.
- SK and ESM normalization matches the current circuit implementation.
- The final C2 proof exposes the current C2 public interface.
- Every coefficient is checked by exactly one chunk proof.
- Every chunk index is present exactly once.
- Every chunk proof is bound to the matching base proof.
- C2 to C3 and C2 to C4 commitments remain unchanged unless a migration is approved.
- Peers receive one signed C2a proof and one signed C2b proof.
- `NodeFold` receives the same semantic C2 commitments as it receives today.
- All proof and witness artifacts use the Nargo beta.26 and Barretenberg 5.1.0 compatibility unit.

## Validation Plan

### Noir tests

Test valid and invalid cases for:

- Secret commitment mismatch.
- Secret consistency mismatch.
- Chunk commitment mismatch.
- Chunk index mismatch.
- Share range failure.
- Parity failure.
- Missing chunk.
- Duplicate chunk.
- Reordered chunk.
- Chunk from another base proof.
- Incorrect final C2 public outputs.

### Rust tests

Add tests for:

- Preset-derived chunk sizes and counts.
- Witness chunk ordering.
- Commitment equivalence with Noir.
- Public-signal field counts.
- Accumulator request ordering.
- Duplicate and missing chunk rejection.
- Final C2 proof routing and circuit validation.
- Crash and retry behavior for pending chunk work.

### End-to-end tests

Run the complete path:

```text
C2 base + chunks
  -> C2 recursive accumulator
  -> C2abFold
  -> C3abFold / C4abFold
  -> NodeFold
  -> NodesFold
  -> DkgAggregator
```

Run at minimum:

- `pnpm noir:test`
- `pnpm rust:test`
- `pnpm generate:verifiers --check`
- `pnpm check:invariants`
- `pnpm check:docs`
- The relevant `pnpm test:integration <name>` scenarios

## Main Risks

1. The base circuit may remain large because it still receives the complete witness and computes
   complete party commitments.
2. The final accumulator proof may not match the ZK proof type expected by `c2ab_fold`.
3. New local proof types can affect durable event and wire compatibility.
4. A weak chunk index or coverage binding can allow omitted, duplicated, or replayed coefficient
   ranges.
5. New circuits increase artifact count and build time, especially when all presets and committees
   are compiled.

## Completion Criteria

The work is complete only when:

- Secure C2a and C2b proving costs improve against the recorded baseline.
- The full current DKG proof flow passes without changes to external C2 proof multiplicity.
- C2 to C3, C2 to C4, C2ab, NodeFold, and on-chain aggregation checks pass.
- All generated artifacts come from Nargo beta.26 and Barretenberg 5.1.0.
- Protocol invariants, flow traces, circuit versioning, and benchmark reports are updated.
