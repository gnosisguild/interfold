# Avail and VectorX Reconstruction Audit

## Purpose

This audit controls the reconstruction of `feat/avail-vectorx-data-availability` onto a clean branch
from `origin/main`. The source branch remains the tested reference. The reconstruction must preserve
required protocol behavior without retaining obsolete Sepolia repair code.

## Fixed review range

| Item              | Revision                                   |
| ----------------- | ------------------------------------------ |
| Baseline          | `10ac245af1ecb8873a332bb188dad2972903ae18` |
| Tested source     | `48e6cdc7657259189588f9752ce5d5377eb1844e` |
| Clean branch base | `10ac245af1ecb8873a332bb188dad2972903ae18` |

The source branch contains 64 commits and changes 259 files. Later commits correct behavior from
earlier commits. Therefore, the reconstruction uses final behavior as its unit of review. Commit
history provides provenance only.

## Mainnet cutover facts

The review confirmed these mainnet facts before reconstruction:

- Requests are paused.
- `activeE3Count` is zero.
- `unreleasedCommitteeCount` is zero.
- Existing operator registration and bonding state remains on Ethereum.
- A fresh node database does not reset contract state.

Operators must preserve signer keys, network identity, configuration, and secrets. Operators must
move the old database to a backup location instead of deleting it.

## Decision rules

Keep code when it provides one of these properties:

- Correct execution of the final secure BFV and Avail flow.
- Recovery after a future mainnet crash or restart.
- Idempotent handling of repeated events or effects.
- Verification of commitments, proofs, deadlines, or finalized chain state.
- Exact deployment, artifact, or release binding.

Remove code when all of these statements are true:

- The code only interprets data from an obsolete Sepolia or unpublished node build.
- A node that starts with an empty database cannot enter the path.
- No supported same-version restart uses the path.
- A focused regression test proves that removal preserves the final flow.

Do not remove generic recovery because mainnet currently has no active E3. A future mainnet E3 must
survive a node restart at every protocol stage.

## Required production slices

| Slice                | Required result                                                           | Status                     |
| -------------------- | ------------------------------------------------------------------------- | -------------------------- |
| Secure committee key | Publish bounded chunks and verify the C5-backed commitment                | Keep                       |
| Data availability    | Verify exact Avail bytes through VectorX on Ethereum                      | Keep                       |
| CRISP input          | Commit first, finalize after availability proof, and block omitted inputs | Keep                       |
| Aggregate output     | Bind the RISC Zero result to the Avail object and Ethereum state          | Keep                       |
| Runtime recovery     | Resume only valid same-version work after restart                         | Keep generic recovery only |
| Consumers            | Keep the SDK, server, indexer, templates, and node runtime consistent     | Keep                       |
| Activation           | Upgrade while paused and drained, then fence obsolete node software       | Keep                       |

## Issue ledger

| ID    | Symptom                                                                                                                     | Root cause                                                                                                   | Required final control                                                                                                                                                      | Mainnet decision        | Evidence                                                                                                   |
| ----- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------- |
| AV-01 | Secure committee key transaction exceeded the client transaction-size limit                                                 | The original transport put the complete secure key in one Ethereum transaction                               | Bounded event chunks plus C5 commitment verification after reassembly                                                                                                       | Keep                    | `RegistrySortitionLib.publishCommitteePublicKeyChunk`; `DataAvailabilityCoordinator`; registry chunk tests |
| AV-02 | A content hash alone did not prove that bytes remained available                                                            | Content addressing proves integrity, not publication or continued retrieval                                  | Avail publication plus an Ethereum-verified VectorX receipt before protocol use                                                                                             | Keep                    | `AvailVectorXDataAvailabilityVerifier`; `e3-data-availability`; DA verifier tests                          |
| AV-03 | A vote could not wait for VectorX inside one wallet action                                                                  | VectorX finalization is asynchronous                                                                         | Commit the proof first, then let a durable worker publish to Avail and finalize the receipt                                                                                 | Keep                    | `CRISPProgram.publishInput`; `finalizeInput`; `AvailabilityService`                                        |
| AV-04 | Computation could omit an input that had committed but had not reached Avail                                                | The input tree and availability state advanced at different times                                            | Reserve the leaf, count pending inputs, and reject computation until every commitment finalizes                                                                             | Keep                    | `pendingInputCount`; `CRISPProgram.verify`; input-availability tests                                       |
| AV-05 | Rust and Solidity rejected the same input envelope                                                                          | Rust decoded flat ABI parameters as a wrapped tuple                                                          | Use parameter encoding and decoding at every boundary                                                                                                                       | Keep                    | `decode_input_envelope`; ABI regression test                                                               |
| AV-06 | Standard tests missed the secure-key size failure                                                                           | Local and CI paths used the insecure BFV preset                                                              | Pin exact secure artifacts and require the reproducible RISC Zero Docker image                                                                                              | Keep                    | circuit provenance scripts; `methods/guest-builder.Dockerfile`; `ImageID.sol`                              |
| AV-07 | Restart could repeat, omit, or revive work                                                                                  | Some state existed only in actor memory, and duplicate replay was not always idempotent                      | Persist assemblies and jobs, replay from durable cursors, and deduplicate effects                                                                                           | Keep generic paths      | runtime recovery tests and flow traces 04, 06, and 08                                                      |
| AV-08 | A reverted Ethereum write could be recorded as successful                                                                   | The CRISP helpers returned a mined receipt without checking its status                                       | Require a successful receipt and simulate the exact write before submission                                                                                                 | Keep                    | `examples/CRISP/crates/evm_helpers`; publication worker tests                                              |
| AV-09 | Old Sepolia state could retain a request context after its terminal event had already passed the saved cursor               | Intermediate test binaries produced an inconsistent projection                                               | Start production nodes from clean data and let ordinary cursor-based replay handle future same-version restarts; do not ship a permanent chain-query repair pass            | Drop old-state repair   | source commit `837efd89c`; clean-start activation procedure                                                |
| AV-10 | A slow valid Avail submission could be cancelled and submitted twice                                                        | The worker's 120-second outer timeout was shorter than Avail's 380-second inner bounds                       | Use an eight-minute outer bound and durable state before each next phase                                                                                                    | Fixed in reconstruction | `AvailabilityService::process`; worker documentation                                                       |
| AV-11 | Valid but abandoned staging requests could grow the server database without limit                                           | The server duplicated ciphertext bytes and signed multiple uncommitted inputs per slot                       | Store one content-addressed copy, allow one uncommitted promise per slot, enforce a byte cap, and compact terminal jobs                                                     | Fixed in reconstruction | `AvailabilityService::store_new_job_with_object`; storage regression tests                                 |
| AV-12 | Old Sepolia repair state would ship as a permanent migration                                                                | Intermediate nodes wrote projections that no published mainnet node needs                                    | Remove the one-time projection migration; retain ordinary same-version restart recovery                                                                                     | Drop migration          | clean diff against source commits `6ead50c1a`–`b2f2e26f6`                                                  |
| AV-13 | A reentrant E3 program could claim a randomness treasury credit before the request transferred tokens into protocol custody | Request accounting ran before external program validation and before `transferFromExact`                     | Make `request` non-reentrant and record payment only after the exact token transfer succeeds                                                                                | Fixed in reconstruction | `Interfold.request`; pricing and request tests                                                             |
| AV-14 | The CRISP indexer could record a Merkle-root update after the Ethereum transaction reverted                                 | The helper waited for a receipt but did not inspect its status                                               | Reject every mined receipt whose status is not successful                                                                                                                   | Fixed in reconstruction | `set_merkle_root`; CRISP integration tests                                                                 |
| AV-15 | Protocol-generation 3 software could be published under the already released `0.14.0` identity                              | The source branch changed the wire protocol without preparing a new release version                          | Publish the production binary under a new SemVer and bind that exact version, protocol, generation, artifacts, and image ID in the activation plan                          | Release gate            | `protocol-release.toml`; node release scripts; activation validation                                       |
| AV-16 | Mainnet could continue accepting the deployed bootstrap mock after data availability became mandatory                       | The non-upgradeable bootstrap program predates `verifyDataAvailability`                                      | Retire the bootstrap program in the secure-CRISP governance batch and prove after execution that it cannot admit new requests; preserve existing E3 snapshots               | Fixed in reconstruction | `Interfold.unregisterE3Program`; secure activation preparation and validation                              |
| AV-17 | A crash during staging could leave either an unusable job or an unowned object that permanently consumed the storage limit  | The content-addressed object and its durable recovery job were separate sled writes                          | Admit the object and job in one cross-tree transaction before returning an availability attestation                                                                         | Fixed in reconstruction | `AvailabilityService::store_new_job_with_object`; atomic-admission regression test                         |
| AV-18 | The generic indexer retained every committee-key chunk after it had stored the verified key                                 | The durable assembly was marked complete without releasing its large temporary byte arrays                   | Compact a completed or rejected assembly to a small replay marker after verification                                                                                        | Fixed in reconstruction | `mark_public_key_assembly`; indexer compaction regression test                                             |
| AV-19 | Nodes rejected the upgraded Interfold ABI during startup tests                                                              | The bootstrap-program retirement event was absent from the Rust event catalog                                | Add `E3ProgramUnregistered(address)` to the current Interfold catalog and retain the exact ABI-drift test                                                                   | Fixed in reconstruction | `event_decoding::catalog`; `every_watched_contract_catalog_matches_its_current_abi`                        |
| AV-20 | Contract coverage failed on different committee tests across repeated runs                                                  | Test helpers advanced by a relative constant instead of the request's VRF-derived deadline                   | Advance to the exact on-chain committee deadline and ensure the mock fulfillment is in a later timestamped block                                                            | Fixed in reconstruction | Registry, pricing, slashing, and lifecycle test helpers; full 770-test coverage run                        |
| AV-21 | The template completed its E3 test but reported a failed job while stopping its background services                         | The harness based success on whichever concurrent process exited first                                       | Make the named `TEST` process authoritative; keep terminating the EVM, nodes, server, miner, and program after that process finishes                                        | Fixed in reconstruction | `templates/default/scripts/test_integration.sh`; full template integration run                             |
| AV-22 | Governance could resume requests before enough upgraded nodes can form every advertised committee                           | Updating the release policy invalidates cached eligibility until each operator acknowledges the new protocol | Validate release-ready operator capacity before generating the resume transaction; require the largest production committee and permit a named smaller gate only on Sepolia | Verified                | Mainnet-fork activation left requests paused and rejected resume with 0 of 19 release-ready operators      |

## External review reconciliation

The external review inspected the source branch only through `46abb807a`. Later source commits and
this reconstruction correct several findings. This table preserves every reported concern so that a
later fix does not erase its audit trail.

| Finding                                                                   | Reconstruction result         | Evidence or decision                                                                                                                                                                     |
| ------------------------------------------------------------------------- | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C1: committee key used the wrong proof commitment                         | Fixed                         | The assembler validates the reassembled key against the C5-backed public-key commitment.                                                                                                 |
| H1: mock programs bypassed the availability callback                      | Fixed                         | Mock and production program interfaces follow the same output-reference path. The activation batch also retires the older deployed bootstrap mock, which cannot implement that callback. |
| H2: Ethereum writes accepted reverted receipts                            | Fixed                         | Input, finalization, output, and Merkle-root helpers require a successful receipt. Exact writes are simulated before submission where supported.                                         |
| H3: an accepted but unavailable input can stop computation                | Accepted fail-closed behavior | The contract must not remove or omit a committed vote. The E3 remains blocked and eventually fails if the availability proof never arrives.                                              |
| H4: generated ABI and TypeChain output could drift                        | Verification gate             | Contract compilation regenerates types, and package and integration tests consume the generated interfaces. Generated artifacts must be clean after the gate.                            |
| M-NEW-1: old recovery state needed head and finalized reconciliation      | Removed from release scope    | This was a repair for inconsistent Sepolia projections. Mainnet has no E3s, nodes start with clean protocol data, and future restarts use ordinary durable-cursor replay.                |
| M-NEW-2: every recovered round created a server RPC poller                | Fixed                         | One server watchdog reuses one provider, tracks all unfinished deadlines, and exits after the finite set drains.                                                                         |
| M-NEW-3: client recovery polling was unbounded and ignored tab visibility | Fixed by policy               | One client loop uses capped exponential backoff, pauses while hidden, and resumes immediately when visible. It continues while the round remains relevant.                               |
| M1: documentation described the old flow                                  | Fixed                         | Flow trace 08 and CRISP, SDK, support, and operator documentation describe commit then availability finalization.                                                                        |
| M2: the core output-reference event was not handled                       | Fixed                         | The event catalog, runtime handler, indexer, and recovery paths consume the output reference.                                                                                            |
| M3: SDK key assembly had no bound or cleanup                              | Fixed                         | The SDK enforces the registry chunk bound and clears completed or invalid assemblies.                                                                                                    |
| M4: object size was checked after commitment                              | Fixed                         | Staging rejects oversized bytes before it signs or submits an Ethereum commitment.                                                                                                       |
| M5: a crash after finalized Avail submission can republish the same bytes | Accepted cost-only limitation | Content hashes and receipt verification preserve correctness. A crash in this narrow window can pay for a duplicate Avail publication but cannot change the accepted object.             |
| M6: routes exposed raw provider errors                                    | Fixed                         | Routes return classified, redacted errors. Logs do not expose provider credentials.                                                                                                      |
| M7: committee-key assembly existed only in memory                         | Fixed                         | Chunk assembly is durable and idempotent across restart and duplicate logs.                                                                                                              |
| M8: deadline recovery could compute without a verified committee key      | Fixed                         | Recovery activates a round only after the generic indexer has stored a commitment-verified committee key.                                                                                |
| M9: input retrieval continues after the voting window                     | Required behavior             | A vote committed before the cutoff may finalize until the compute deadline. Terminal cleanup stops later work and removes large bytes.                                                   |
| L1: VectorX range indexing was ambiguous                                  | Verified                      | SP1 Vector proves `(start, end]`; the adapter maps index zero to `start + 1`. A real Sepolia gate must confirm the deployed bridge behavior.                                             |
| L2: the local mock is trusted                                             | Local-only                    | Public-chain deployment rejects mock DA configuration. The mock exists only for deterministic local tests.                                                                               |
| L3: an expelled member can still finish a key upload                      | Intentional repair path       | Any valid chunk only reconstructs the already proven C5 commitment. Allowing completion avoids stranding a key after membership changes.                                                 |
| L4: the verified-concerns index described stale behavior                  | Fixed                         | The index and flow traces describe member-gated chunk repair and the final C5 commitment check.                                                                                          |
| L-NEW-1: deferred mainnet wiring could activate an unsafe program         | Fixed                         | Mainnet preparation and resume validate the exact CRISP program, verifier, DA verifier, and paused state.                                                                                |
| L-NEW-2: a stale round response could replace a newer round               | Fixed                         | The client confirms the current round again before it commits fetched state.                                                                                                             |
| L-NEW-3: stale local vote recovery could create a new ciphertext          | Fixed                         | Recovery reuses the exact stored envelope and refuses unsafe legacy recovery.                                                                                                            |
| L-NEW-4: dashboard ABI omitted new events                                 | Fixed                         | Dashboard and SDK ABIs include the chunk and output-reference events.                                                                                                                    |
| L-NEW-5: replay produced misleading errors                                | Fixed                         | Expected stale or terminal replay outcomes are classified and suppressed.                                                                                                                |
| L5: old nodes could join the new flow                                     | Fixed                         | Protocol version 3 isolates the wire network and the on-chain release policy rejects obsolete versions.                                                                                  |
| L6: timing validation was static or optional                              | Fixed                         | Upgrade preparation validates live timing relationships and rejects malformed blank options.                                                                                             |
| L7: unrelated local files entered the branch                              | Fixed in reconstruction       | Historical drain helpers, local notes, and stale Sepolia records are absent.                                                                                                             |
| L8: one bad key candidate could block all candidates                      | Fixed                         | Candidate tracking is per member; another honest member can supply a valid committed key.                                                                                                |

## Accepted first-release limitations

- Avail proves publication and VectorX proves the Ethereum receipt. Neither system guarantees that
  every third-party RPC remains online. The server retries independent endpoints and fails closed.
- An accepted input that never receives a valid availability receipt cannot be removed. This is
  intentional: removal after commitment would let an operator omit a valid vote.
- A crash after Avail finality but before the local durable state update can publish the same bytes
  again and pay a second Avail fee. The duplicate cannot alter the committed bytes or proof result.
- Client result polling has no fixed maximum while the round remains relevant. It pauses while the
  page is hidden and uses a capped exponential delay while visible.
- Terminal job metadata remains for audit and idempotency. Terminal compaction removes the large
  staged objects and proofs.
- The first CRISP deployment pays Avail publication costs from its availability-service account. The
  service publishes only after it observes an accepted Ethereum input commitment, but the
  application must fund and monitor this account according to its maximum input volume.
- The availability service checks the Noir proof and the hash of the received ciphertext before it
  publishes to Avail. The RISC Zero Secure Process performs the final check that the ciphertext
  bytes reproduce the Noir-proven BFV commitment. A mismatched object is excluded from the tally and
  cannot change the result, but it can consume one Avail publication and relay capacity. Rate limits
  contain this cost; recomputing the BFV commitment before the service signs is a future economic
  hardening option.

## Commit disposition

Each source commit receives one of these dispositions:

- `REBUILD`: reproduce its surviving behavior inside a production slice.
- `TEST`: retain only focused regression coverage.
- `OPS`: keep deployment evidence outside production runtime changes.
- `DROP`: omit behavior that only repairs obsolete unpublished state.
- `SUPERSEDED`: omit the intermediate implementation and retain its corrected final form.

## Source commit ledger

This table records every source commit. `REBUILD` means the final corrected behavior remains.
`SUPERSEDED` means a later implementation replaces the commit. `DROP` means the change only records
or repairs obsolete Sepolia state. `TEST` and `OPS` are retained only where they verify or operate
the final design.

|   # | Commit      | Subject                                          | Disposition | Reconstruction decision                                                             |
| --: | ----------- | ------------------------------------------------ | ----------- | ----------------------------------------------------------------------------------- |
|   1 | `3d9254478` | Refresh Sepolia rehearsal deployment             | DROP        | Fresh Sepolia deployment will produce new records.                                  |
|   2 | `68e46eaf3` | Update Sepolia faucet references                 | DROP        | Unrelated to the protocol change.                                                   |
|   3 | `69f66a744` | Allow secure committee public keys               | SUPERSEDED  | Keep the final bounded chunk transport, not the intermediate 512 KiB transaction.   |
|   4 | `14b18c89e` | Record Sepolia registry hotfix                   | DROP        | Historical testnet transaction only.                                                |
|   5 | `d104494fc` | Default SelfRegistry rounds on-chain             | REBUILD     | Generic CRISP round behavior.                                                       |
|   6 | `3e99a329d` | Harden Sepolia CRISP recovery                    | SUPERSEDED  | Keep only recovery behavior that applies to a fresh production round.               |
|   7 | `c39efca6c` | Add proof-backed Avail availability              | SUPERSEDED  | Replaced by the complete two-stage flow.                                            |
|   8 | `1d832f20a` | Complete Avail availability flow                 | REBUILD     | Core Avail, VectorX, input, output, and recovery behavior.                          |
|   9 | `442aa03fd` | Refresh reproducible RISC Zero artifacts         | OPS         | Keep the Docker-built image ID and reproducible copy path.                          |
|  10 | `35abc1c95` | Validate chunked committee keys                  | REBUILD     | Use the C5 commitment, not the incompatible C0 commitment.                          |
|  11 | `d662ccf75` | Align input envelope ABI encoding                | REBUILD     | Flat parameter encoding is the contract ABI.                                        |
|  12 | `039923825` | Contain CRISP recovery failures                  | REBUILD     | Keep bounded, fail-closed recovery.                                                 |
|  13 | `984938324` | Ignore stale failure-stage reads                 | REBUILD     | A stale RPC response cannot regress lifecycle state.                                |
|  14 | `4db7dcb76` | Recover failed-E3 settlement                     | REBUILD     | Future crashes must not strand settlement.                                          |
|  15 | `ab5abd137` | Import settlement selector trait                 | TEST        | Compile coverage for the settlement path.                                           |
|  16 | `fa7b24c97` | Clarify compute retry semantics                  | OPS         | Keep accurate operator documentation.                                               |
|  17 | `ec3c981bd` | Harden Sepolia rehearsal records                 | DROP        | Old deployment data only.                                                           |
|  18 | `eece899dc` | Merge recovery hardening                         | SUPERSEDED  | No independent behavior.                                                            |
|  19 | `97221a2b6` | Merge rehearsal records                          | DROP        | No independent production behavior.                                                 |
|  20 | `309cc9714` | Update round fixtures for voting deadline        | TEST        | Fixtures match the final deadline model.                                            |
|  21 | `140a543d3` | Replace snapshot timing race with flush barriers | TEST        | Deterministic runtime test synchronization.                                         |
|  22 | `aac33da16` | Preserve snapshot cursor assertion               | TEST        | Guards exact replay progress.                                                       |
|  23 | `ce0e5d8fd` | Make local EVM time advancement monotonic        | TEST        | Local tests must not create impossible chain time.                                  |
|  24 | `362244df5` | Discover rounds created after page load          | REBUILD     | Required client behavior.                                                           |
|  25 | `35e3e6fe1` | Handle absent governance batch                   | OPS         | Deployment preparation must fail clearly when no action exists.                     |
|  26 | `f8541cacb` | Allow deferred protocol wiring                   | OPS         | Required for a paused DAO upgrade; guarded on mainnet.                              |
|  27 | `56e32ce6d` | Compile Avail verifier for deployment            | OPS         | Deployment must use a compiled verifier artifact.                                   |
|  28 | `fb87842f4` | Read secure activation state correctly           | OPS         | Validation reads the proxy state that governance changes.                           |
|  29 | `61016f751` | Record secure CRISP activation                   | DROP        | Old Sepolia state.                                                                  |
|  30 | `1ed16d991` | Track secure CRISP addresses                     | DROP        | Old Sepolia state.                                                                  |
|  31 | `6ead50c1a` | Prune terminal recovery contexts                 | REBUILD     | Keep runtime cleanup; drop the one-time projection migration.                       |
|  32 | `c06996eb6` | Import recovery projection helpers               | DROP        | Only supported the removed migration test.                                          |
|  33 | `b2f2e26f6` | Give recovery events distinct timestamps         | DROP        | Only supported the removed migration test.                                          |
|  34 | `837efd89c` | Reconcile recovery with finalized chain state    | DROP        | Repairs obsolete Sepolia projection state and adds a startup RPC dependency.        |
|  35 | `856fbd66a` | Disambiguate recovery checkpoint repository      | REBUILD     | Prevents reading the wrong durable state.                                           |
|  36 | `b23edc4f9` | Classify parameterized terminal reverts          | REBUILD     | Typed terminal errors stop pointless retries.                                       |
|  37 | `1245c9d41` | Ignore completed recovery history                | REBUILD     | Completed E3s must not restart work.                                                |
|  38 | `3a82ca8f4` | Distinguish completed local events               | TEST        | Covers the completed-history rule.                                                  |
|  39 | `881e0bf9f` | Keep round discovery live across wallet prompts  | REBUILD     | Final client uses bounded, visibility-aware polling.                                |
|  40 | `a672e8695` | Deduplicate failure settlements                  | REBUILD     | At-least-once events cannot create duplicate writes.                                |
|  41 | `f06f05e5e` | Redact support script failures                   | REBUILD     | Secrets and raw provider errors stay out of logs.                                   |
|  42 | `40a424065` | Ignore unrelated CRISP rounds                    | REBUILD     | One application does not process another application's E3.                          |
|  43 | `9e5056c56` | Update secure recovery flow                      | OPS         | Keep only statements that match the reconstruction.                                 |
|  44 | `980ac4b3d` | Make secure CRISP cutover resumable              | OPS         | A failed governance preparation can resume safely.                                  |
|  45 | `c16a1f4b9` | Preserve app focus after reload                  | TEST        | Client regression coverage.                                                         |
|  46 | `9ff32d065` | Await wallet session restoration                 | TEST        | Client regression coverage.                                                         |
|  47 | `c4c86196d` | Retain late round state                          | REBUILD     | Delayed indexing cannot erase a real round.                                         |
|  48 | `46abb807a` | Repair stale active-job counters                 | SUPERSEDED  | Drop old-counter repair; keep idempotent committee recording to prevent recurrence. |
|  49 | `ab3bd7635` | Address availability review findings             | REBUILD     | Receipt checks, bounds, error mapping, durable assembly, and SDK limits.            |
|  50 | `71aa9bef5` | Validate ciphernode Docker workspace             | TEST        | Image build must include every workspace member.                                    |
|  51 | `b2ba6e705` | Use canonical Noir workspace artifact            | TEST        | Prevents source/artifact drift.                                                     |
|  52 | `507727bab` | Allow local server without Etherscan key         | REBUILD     | Etherscan is optional outside holder discovery.                                     |
|  53 | `df7c602d3` | Consume chunked keys in the template             | REBUILD     | Generated applications must match the registry ABI.                                 |
|  54 | `bcf3eb1a4` | Expose a Node ESM WASM initializer               | REBUILD     | Removes ambiguous runtime initialization.                                           |
|  55 | `a15c56a0d` | Allow local EVM time travel                      | TEST        | Restricted to local chain IDs.                                                      |
|  56 | `d77144216` | Coordinate mock data                             | TEST        | Local E2E uses one deterministic object source.                                     |
|  57 | `2f43d8da4` | Cover chunked key and DA retrieval               | TEST        | Cross-layer integration coverage.                                                   |
|  58 | `277a5164e` | Use a dedicated Boundless IPFS gateway           | OPS         | Prevents shared-gateway rate limits from blocking proving.                          |
|  59 | `8b3034378` | Select the image from the local CLI revision     | OPS         | The submitted image must match the checked-out source.                              |
|  60 | `bfb1c7224` | Stabilize input deadline boundary                | TEST        | Tests the exclusive commitment and inclusive finalization cutoffs.                  |
|  61 | `2960e6952` | Size Boundless offers for secure compute         | OPS         | Secure-8192 proving needs realistic offer limits.                                   |
|  62 | `df0eaa924` | Restore Avail and decryption recovery            | REBUILD     | Both paths are required after a future restart.                                     |
|  63 | `151e4189a` | Discard obsolete DKG replay work                 | REBUILD     | Lifecycle-gated compute replay prevents stale DKG work.                             |
|  64 | `48e6cdc76` | Preserve decryption verification on replay       | REBUILD     | The DKG filter must not discard C6 decryption work.                                 |

## Release gates

The clean branch is not ready until all gates pass:

1. Compare proxy storage layouts with the live implementations.
2. Execute the exact governance batch on a mainnet fork.
3. Start a node from an empty database and reconstruct current mainnet state.
4. Restart nodes at every E3 boundary and verify deterministic recovery.
5. Run the secure-8192 CRISP flow with real Avail, VectorX, and Boundless.
6. Verify that the source revision, circuits, verifiers, ELF, image ID, SDK, and node release agree.
7. Verify that obsolete binaries cannot join the activated protocol.
8. Publish protocol version 3 under a new SemVer. Do not reuse the released `0.14.0` identity.

## Verification evidence

- The fixed storage-layout gate passed for every upgraded proxy.
- The exact 12-action secure-CRISP governance batch executed successfully against a fork of the
  deployed mainnet state at block `25889890`.
- Independent post-upgrade validation confirmed all three secure BFV routes, the CRISP program, the
  Ethereum Avail/VectorX verifier, protocol version 3, bootstrap-program retirement, and the paused
  state.
- The resume script refused to produce an unpause transaction because no operator had acknowledged
  protocol version 3. Production requires 19 release-ready operators because 19 is the largest
  configured secure committee. This refusal is the intended safety gate.
- The full local CRISP browser flow passed with mock DA. It covered key chunk assembly, vote proof
  commitment, delayed availability finalization, exact input recovery, computation, output binding,
  decryption, and terminal cleanup.
- The RISC Zero guest was rebuilt twice with Docker. Both builds produced image ID
  `0xd39da4f3d8944740fa787691c941d40aa9762a6156f025a425e414f9a5ff49c0`, which matches `ImageID.sol`.
- No mainnet or Sepolia transaction was sent during these checks.
