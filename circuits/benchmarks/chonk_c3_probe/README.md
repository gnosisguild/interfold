# Real C3 Chonk Experiment

This fixture evaluates replacing the sequential C3a/C3b fold chain with Chonk while preserving the
existing C3, NodeFold, NodesFold, C5, and DKG aggregator boundaries. The experiment was run on a
local macOS runner with the insecure-512 preset.

## Executive Result

The final comparison used the same small committee and the same complete multi-node pipeline. All
three runs passed `NodeFold -> NodesFold -> C5 -> dkg_aggregator`, including proof verification:

| Metric | Classic sequential | Batched Chonk | Leaf-stacked Chonk |
| --- | ---: | ---: | ---: |
| Committee | N=19, H=10 | N=19, H=10 | N=19, H=10 |
| C3 leaf proofs per node | 72 | 72 | 72 |
| C3 fold shape per chain | 36 sequential folds | 2 x 18-leaf chunks | 4 x 9-leaf stacks |
| Full test time | 4340.90s | 3592.07s | 4823.19s |
| Wall-clock duration | 1h 12m 20.90s | 59m 52.07s | 1h 20m 23.19s |
| Change vs classic | baseline | -748.83s (-17.25%) | +482.29s (+11.11%) |

The batched path was `1.2085x` faster than the classic path. The leaf-stacked path was `1.1110x`
slower than classic and `1.3427x` slower than batched Chonk in this full-pipeline measurement.

The headline results include complete fixture-generation cost, C4 proofs, C3 aggregation, NodeFold,
NodesFold, C5, and DKG aggregation. They are end-to-end results, not isolated C3 prover
microbenchmarks. The paths were run independently with equivalent generated committee material.

## Reproduction

Build the active artifacts for the comparison committee:

```bash
pnpm build:circuits --preset insecure-512 --committee small
pnpm generate:verifiers --write
pnpm check:committee
pnpm check:verifiers
```

Run Chonk:

```bash
cargo test -p e3-zk-prover --test chonk_node_fold_e2e_tests \
  chonk_c3_flows_through_small_multi_node_dkg_aggregator -- --ignored --nocapture
```

Run leaf-stacked Chonk:

```bash
cargo test -p e3-zk-prover --test chonk_node_fold_e2e_tests \
  chonk_c3_leaf_flows_through_small_multi_node_dkg_aggregator -- --ignored --nocapture
```

Run the classic control:

```bash
cargo test -p e3-zk-prover --test chonk_node_fold_e2e_tests \
  classic_c3_flows_through_small_multi_node_dkg_aggregator -- --ignored --nocapture
```

The tests are intentionally ignored because each full run is long. Each small-committee run
generated 10 correlated honest-node DKG states, 72 C3 leaf proofs per node, and 10 C4 proof pairs.
The measured classic, batched, and leaf-stacked runs took approximately 72, 60, and 80 minutes.

The standalone probe can be run with compiled probe artifacts and optional Rust-generated leaf
fixtures:

```bash
pnpm --dir packages/interfold-sdk probe:chonk-c3
```

Set `CHONK_C3_COMMITTEE` to `minimum`, `micro`, or `small`. Set `CHONK_C3_LEAF_FIXTURES` and
`CHONK_C3_OUTPUT` to pass leaves through the JSON boundary and persist tube fixtures.

## Chonk Pipeline

For batched Chonk, the probe performs the following steps:

1. `c3_batch_app` verifies a batch of real `ShareEncryption` proofs and emits the sparse `pk/msg/ct`
   accumulator.
2. `c3_init_kernel`, `c3_tail_kernel`, and `c3_hiding_kernel` form the Chonk kernel chain.
3. Chonk proves the kernel chain and verifies the resulting Chonk proof.
4. `c3_tube` verifies the Chonk proof and exposes the accumulator using the six-field C3 fold prefix
   and the existing `c3_fold` public-input width.
5. `c3_chunk_fold` verifies two RollupHonk tube proofs, merges their sparse accumulator tails, and
   emits one ordinary C3 fold-shaped proof.
6. `c3ab_fold` verifies the ordinary C3a and C3b chunk-fold proofs and returns the existing C3ab ABI.

For leaf-stacked Chonk, `c3_leaf_app` verifies one real `ShareEncryption` proof per step, and the
leaf-specific init/inner/tail/hiding kernels compactly carry the sparse state through a nine-leaf
stack. For the small committee, each chain runs four Chonk/tube fixtures. Each pair is closed by
`c3_chunk_fold`, and the two resulting ordinary proofs are merged by `c3_leaf_chunk_fold` before
`c3ab_fold`.

The classic control retains the original sequential Rust fold helper and uses
`c3ab_fold_sequential` for the final C3a/C3b wrapper. The production request path passes no Chonk
override, so the default behavior remains sequential while the experimental seams are evaluated.

## Why Two Chunks

The first attempt changed `c3ab_fold` to verify arrays of Chonk-backed C3 proofs directly. Barretenberg
rejected the resulting root Rollup circuit during VK generation:

```text
Root rollup must accumulate two IPA proofs.
```

The v5.1.0 root Rollup verifier requires exactly two nested IPA claims. A single C3a or C3b root
cannot close all of the small committee's non-local leaves in one Chonk proof. Batched mode therefore
uses two 18-leaf Chonk batches per chain and one `c3_chunk_fold` root. Leaf mode uses four 9-leaf
stacks, two `c3_chunk_fold` roots, and the ordinary `c3_leaf_chunk_fold` merge. Every root remains at
the supported two-claim boundary and the final proof retains the ordinary ABI consumed by NodeFold.

The batch size is derived as:

```text
C3_BATCH_SIZE = (C3_SLOTS - L_THRESHOLD) / 2
```

For the measured small committee this is 18 leaves per batch: 36 non-local leaves per chain split
into two chunks. Leaf mode uses `C3_LEAF_STACK_SIZE = 9`, producing four stacks per chain.

## Correctness Coverage

The integration validates more than proof verification:

- Every Chonk/tube fixture exposes exactly 480 RollupHonk proof fields.
- Tube public inputs retain the expected `6 + 3*C3_SLOTS` width.
- Leaf, kernel, tube, chunk-fold, and final C3 VK hashes are bound in the public inputs.
- C3a and C3b use the same replacement tube VK hash.
- Slot indices are unique, range-checked, and preserved through the TypeScript/Rust boundary.
- Each chunk has zero accumulator values outside its assigned non-local slots.
- Message accumulator values are checked against the corresponding C2a/C2b proofs.
- The final sparse accumulator is merged without allowing duplicate non-zero slots.
- The complete NodeFold, NodesFold, C5, and EVM-targeted DKG aggregator proofs verify.

## Other Measurements

The minimum-committee smoke tests are useful for functional coverage but are not the headline
comparison because their harnesses differ:

| Test | Result |
| --- | ---: |
| Chonk correlated NodeFold, minimum | 168.79s |
| Classic sparse self-slot NodeFold regression, minimum | 120.74s |

The Chonk probe also had an earlier isolated minimum fixture with three active C3 slots. It measured
the Chonk aggregation-plus-tube portion at 10.17s versus 12.80s for sequential folding, but its leaf
generation used different implementations (TypeScript `zk_cli` versus in-process Rust helpers), so
those leaf and total timings are not used for the final claim.

## Implementation Changes

- Added `circuits/bin/recursive_aggregation/c3_chunk_fold`.
- Added the compact leaf-stack circuits and `circuits/bin/recursive_aggregation/c3_leaf_chunk_fold`.
- Added `circuits/bin/recursive_aggregation/c3ab_fold_sequential` and kept the original sequential
  boundary available for control and production fallback.
- Updated `c3ab_fold` to consume ordinary proofs emitted by the chunk-fold adapter.
- Added the real Chonk probe and benchmark circuits under this directory.
- Added `packages/interfold-sdk/scripts/chonk-c3-probe.ts` and the `probe:chonk-c3` script.
- Added optional C3 proof overrides to `node_dkg_fold.rs` while leaving the default path unchanged.
- Added circuit-name, artifact, staging, and VK-manifest support for the new recursive adapters.
- Added correlated single-node and full multi-node Chonk/classic E2E tests.
- Regenerated the insecure-512/small DKG aggregator verifier for the updated public-input manifest.

## Validation

The following checks passed during the experiment:

- `nargo compile` for the new recursive and Chonk probe circuits.
- `pnpm build:circuits --preset insecure-512 --committee small`.
- `cargo fmt --all -- --check`.
- Rust workspace checks covering `e3-events`, `e3-zk-prover`, and `e3-multithread`.
- `cargo test -p e3-zk-prover --lib`: 159 passed.
- `cargo test -p e3-events`: 133 passed.
- Circuit tooling checks: 6 passed.
- Committee and verifier consistency checks.
- Contract compilation and the BFV verifier-router tests: 2 passed.
- `git diff --check`.

The SDK TypeScript check still reports missing generated `@interfold/contracts/types` and
`@interfold/wasm` modules. That is a generated-artifact setup issue and did not affect the Chonk
probe or the Rust E2E results.

## Limitations and Follow-ups

- The final end-to-end comparison covers insecure-512/small only; secure-8192 was not run.
- Batched mode intentionally uses two chunks; leaf mode uses four nine-leaf stacks. Both require the
  non-local slot count to divide evenly.
- The experiment measures full pipeline wall time. A future report can expose per-stage timing from
  `FoldProveStepTiming` and the probe's Chonk/tube timers in one common table.
- The TypeScript probe uses fixed bb.js v5.1.0 field lengths and should be versioned alongside the
  Barretenberg dependency.
- The current NodeFold override is an integration seam. Production still uses sequential C3 folding
  until a deployment decision is made.
