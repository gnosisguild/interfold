# Ciphernode Actor Audit

This audit records the repository-wide review of production `src/actors/` directories performed
for the thin-actor refactor. Read it with [`ARCHITECTURE.md`](ARCHITECTURE.md), which defines the
target, and [`CRATES_ARCHITECTURE.md`](CRATES_ARCHITECTURE.md), which describes the implementation.

## Method

An actor is thin when it has one runtime reason to change: mailbox serialization, routing,
scheduling, lifecycle, or supervision. Line count is evidence, not the definition. Roughly 300
production lines triggers a responsibility review; tests, generated bindings, and cohesive
cryptographic algorithms are judged separately.

The audit checked all nine crates that originally contained `src/actors/`. Before this refactor,
17 production actor modules
were over the review threshold; the largest were `ThresholdKeyshare` (2,198 lines),
`PublicKeyAggregator` (1,591), `ProofRequestActor` (1,344), and
`ThresholdPlaintextAggregator` (1,029). After the refactor, complex actors follow the same
`mod.rs` / `handlers` / `runtime` / `tests` package structure. The largest actor/workflow source is
the 309-line node-fold collector; every other file in those trees is below the review trigger.

## Findings by crate

| Crate | Actors reviewed | Result |
| --- | --- | --- |
| `e3-aggregator` | committee finalizer, keyshare/decryption buffers, public-key aggregator, plaintext aggregator | Aggregation state and deterministic decisions live under `workflow/`; handlers are mailbox entry points and C1/C5/C6/C7/completion effect glue is below actor `runtime/`. |
| `e3-evm` | parser/router/hub, chain gateway, readers, registry/interfold/slashing writers, log fetcher | Provider log fetching moved out of `actors/` to `adapters/`. Complex writers/readers use `handlers` plus `runtime` for provider recovery, streams, preflights, and transaction effects. |
| `e3-keyshare` | encryption/share/decryption collectors, `ThresholdKeyshare` | Collection actors are cohesive. `ThresholdKeyshare` remains the request-local DKG coordinator; all phase-specific effect/correlation code is below `runtime/`, transient async-gap data is grouped in `PendingKeyshareWork`, and persisted protocol state remains in `domain::keyshare_state`. |
| `e3-net` | event buffer/translator, sync manager, document publisher/converter | Admission, readiness, rebroadcast, historical sync, conversion, and DHT/gossip effects are isolated. The actors now own transport ordering and lifecycle rather than document-validation policy. |
| `e3-request` | lifecycle coordinator, E3 router | The router now has the standard package split: mailbox routing in `handlers.rs`, builder/snapshot integration in `runtime/`, and deterministic decisions in `domain/`. |
| `e3-slashing` | accusation manager, commitment consistency checker | `AccusationManager` is a timer/effect shell over `workflow::accusation_voting`; deterministic digest, admission, re-verification, vote, and quorum logic lives below workflow `transitions/`. |
| `e3-sortition` | sortition, ciphernode selector | Sortition message families are below `handlers/`. The selector owns its persisted cache and aggregator-change publication; canonical selection rules remain in `domain/`. |
| `e3-sync` | bootstrap/replay functions and messages | This directory did not contain an Actix actor. It was removed from `actors/`; effectful startup orchestration now has the honest path `runtime/sync/`. |
| `e3-zk-prover` | proof requester, share verifier, C0 verifier, node proof aggregator, ZK worker, commitment links | Proof state and deterministic verification live under `workflow/` or `domain/`; correlations, worker dispatch, signing, and publication are below actor `runtime/`. Pure commitment links moved from `actors/` to `domain/`. |

## State classification applied

- Persisted workflow state: aggregation and keyshare state enums stored through repositories.
- Derivable state: canonical committee/preset caches rebuilt from repositories and replay.
- Ephemeral effect state: correlation IDs, collector addresses, timer handles, early-arrival buffers,
  and in-flight submission guards. These are grouped and named rather than mixed into protocol
  state.
- External authority: EVM contract state. Writer preflights provide cross-restart idempotency where
  no durable local outbox exists.

`Persistable::try_mutate` now accepts the snapshot write into the bounded store mailbox before it
exposes the new value in memory. This does not turn snapshots into an external-effect outbox; the
append-only event log and chain remain the stated authorities. Synchronous `BusHandle` publication
still uses the existing burst-tolerant `do_send` path and is recorded as backpressure debt.

## Deliberate residuals

This refactor does not split files only to satisfy a number. Generated ABI bindings, circuit witness
construction, and cohesive FHE/math algorithms may exceed 300 lines. Large non-actor composition or
infrastructure coordinators—notably `CiphernodeBuilder`, `NetInterface`, and the multithread task
pool—need their own behavior-preserving projects if their responsibilities are changed.

The remaining architectural gap is durable effect intent. Transaction submission and some
cryptographic work still rely on replay plus external preflight instead of a versioned local
intent/result outbox. That is a schema and recovery change, not a safe file-movement refactor, and
must be implemented with migration and crash-matrix tests.
