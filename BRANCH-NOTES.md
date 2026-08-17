# Branch notes — compute input binding and guest provenance

Working notes for `fix/compute-input-binding-and-provenance`. Delete this file before merge, or move
its content into the pull request description.

## What this branch changes

### Security

| Finding                                                                   | Change                                                                                                                                            | Test                                                   |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Guest proved the input root and the computation over unrelated input sets | `ComputeInput` holds only `fhe_inputs`; `process` derives the leaves from the ciphertexts it processed. `with_leaf_hashes` is now `#[cfg(test)]`. | `compute_input::tests` (2 tests)                       |
| SAFE commitment ignored components beyond `c[1]`                          | `bfv_ciphertext_to_greco` rejects any component count other than two.                                                                             | `ciphertext_with_more_than_two_components_is_rejected` |
| CRISP never bound the emitted ciphertext bytes to the stored commitment   | **Not fixed. See "Open decision" below.**                                                                                                         | —                                                      |

`ComputeManager::start_parallel` is removed. It set the final leaves to sub-tree roots, which is
incompatible with deriving leaves from ciphertexts, and it produced a tree of sub-tree roots rather
than the flat input root an E3 program compares against. Every call site passed
`use_parallel = false`, so the path was unreachable. `ComputeManager::new` loses its `use_parallel`
and `batch_size` parameters.

### Build and provenance

- `crates/support/Dockerfile`: `ARG RISC0_TOOLCHAIN` `1.88.0` → `1.91.1`. The old value could not
  build the guest at all — the guest lockfile pins fhe.rs `f2c1d22`, whose workspace declares
  `rust-version = "1.91.1"`.
- `crates/support/methods/build.rs`: `RISC0_USE_DOCKER` is read for its value, not its presence, so
  `RISC0_USE_DOCKER=0` no longer selects the Docker build.
- `crates/support/tests/Elf.sol`: untracked and `.gitignore`d. It held a machine-local absolute path
  (`/home/ace/...`) and nothing consumes it.
- `docs/pages/tutorials/write-e3-program.mdx`: the journal example moved from the stale four-field
  528-byte layout to the current nine-field 1,188-byte layout, and now states that an E3 program
  must compare the input root itself.
- New `pnpm check:image-id` (`scripts/check-image-id.sh`), wired into `.husky/pre-push` and a new
  `check_image_id` CI job.
- New `pnpm check:verifiers`, wired into pre-push and the `build_circuits` CI job.
- New `pnpm provenance:manifest` (`scripts/generate-provenance-manifest.ts`) — the release record.
  It refuses to claim completeness: unresolved fields are listed and `complete` is `false`.
- New docs page `docs/pages/verifying-the-compute-provider.mdx`, linked from the compute-provider
  page and from `write-secure-program.mdx`.
- The `e3-compute-provider` pins in `crates/support/Cargo.toml` and
  `crates/support/methods/guest/Cargo.toml` now carry the rationale for being revisions rather than
  paths, and the rule for bumping them.
- `CRISPProgram.setImageId` / `setRisc0Verifier` document that they cannot replace an accepted
  computation but can fail an E3 in flight.

## Blocker: the guest still runs the old code

**The leaf-derivation fix does not reach the deployed guest on this branch.**

`crates/support` is a separate Cargo workspace, excluded from the root workspace on purpose
(`Cargo.toml:53` — "client needs to be able to build crates/support independently"). It consumes
`e3-compute-provider` through a git pin to a **remote** revision:

- `crates/support/Cargo.toml:45`
- `crates/support/methods/guest/Cargo.toml:16`

Both name `c2097da61b4d07c4ce83840393ff4e9f171eefb4`. So the guest compiles the published crate at
that revision, not the tree. Changing `crates/compute-provider` locally has no effect on the guest
until the pin moves, and the pin can only move to a pushed commit.

That pinned revision is already behind main in a second way: `crates/compute-provider` at `c2097da6`
computes the Merkle depth as `ceil(log2(n))`, without the `.max(1)` that main added.
`one_zero_leaf_matches_solidity_lazy_imt` documents that the minimum depth of one is what matches
the Solidity LazyIMT.

`crates/support/host/src/lib.rs` therefore still calls the **five-argument** `ComputeManager::new`,
because that is the signature at the pinned revision. It is deliberately left alone.

### Sequence to finish the fix

1. Review and merge this branch.
2. Push, then bump both pins to the merge commit.
3. Update the two `ComputeManager::new` call sites in `crates/support/host/src/lib.rs` to the
   three-argument signature.
4. Rebuild the guest: `./scripts/check-image-id.sh --rebuild`.
5. Commit the regenerated `crates/support/contracts/ImageID.sol`, refresh
   `crates/support/contracts/ImageID.stamp.json`, and set `imageIdVerified` to `true`.
6. Redeploy `Risc0BfvCiphertextVerifier` — `imageId` is immutable — and every E3 program that stores
   its own image ID.

## The image ID is stale, and the stamp does not hide it

`crates/support/contracts/ImageID.sol` last changed on 2026-06-23 in `8b81f51c`. The guest and the
journal types changed on 2026-08-06 in `f716d933b` and again on 2026-08-13 in `02ebe4589`. The first
of those replaced a four-field result with the nine-field domain-separated journal. The committed
image ID almost certainly does not correspond to the current tree.

`ImageID.stamp.json` starts the drift ratchet from the current tree so future guest changes are
caught, and carries `"imageIdVerified": false` with the reason. `check:image-id` prints a warning
whenever that flag is false, so a green check is not read as a verified artifact. Only `--rebuild`
clears it, and that is blocked on the pin bump above.

## Open decision: Finding 3, CRISP vote bytes

`CRISPProgram.publishInput` decodes `encryptedVote` and emits it in `InputPublished`, but never
binds those bytes to `encryptedVoteCommitment`. The commitment is a SAFE/Poseidon value over the
ciphertext's CRT limbs, so the contract cannot recompute it from the serialized bytes — there is no
on-chain fix.

Before this branch, a compute provider could route around a mismatched input by supplying the
genuine on-chain leaves while processing only the well-formed ciphertexts. **That escape hatch was
the leaf-derivation defect, and this branch closes it.** With leaves derived from the consumed
ciphertexts, one malformed input makes the round permanently unresolvable: the prover cannot
reproduce the on-chain root, and the failure bills the requester as a `ComputeTimeout`.

Any address in the public census can trigger this for the price of one input transaction. The
mask-vote branch of `circuits/bin/crisp/src/main.nr` asserts only `valid_zero_vote`; the signature
and address checks sit in the `is_mask_vote == false` branch.

**This must be resolved before the pin bump in step 2 above.** Landing the leaf-derivation fix
without it trades a forgery vector for a denial-of-service vector. The options are in the
conversation; none of them is a small contract edit.
