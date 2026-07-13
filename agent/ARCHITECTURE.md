# Interfold Ciphernode Target Architecture

This document defines the target architecture for the Rust ciphernode. It is a design constraint for
new code and the destination for incremental refactors; it is not a claim that every existing module
already complies.

For the current implementation and protocol sequence, read
[`CRATES_ARCHITECTURE.md`](CRATES_ARCHITECTURE.md) and [`flow-trace/`](flow-trace/). When documents
disagree with deployed contracts or executable protocol tests, the contracts and tests win and the
documents must be corrected.

The repository-wide thin-actor findings and deliberate residuals are recorded in
[`ACTOR_AUDIT.md`](ACTOR_AUDIT.md).

## Why Interfold Uses Actors

Interfold is an asynchronous, multi-party cryptographic workflow. A node concurrently receives
chain logs, peer messages, timers, proof results, storage acknowledgements, and operator signals.
Work is partitioned by E3 and must remain responsive while cryptographic jobs run for seconds or
minutes. Actors are a good fit for those concurrency boundaries.

Actors are not the domain model. Putting every rule, state transition, and external effect in an
Actix handler creates large stateful objects that are hard to test and impossible to replay with
confidence.

The governing principle is:

> Actors are concurrency boundaries. Deterministic reducers own protocol and workflow decisions.
> Effect runners perform crypto, storage, network, and chain I/O. Every correctness-relevant
> transition and intent is recoverable.

This means Interfold keeps global choreography between nodes while using explicit, persisted local
orchestration for each E3.

## Sources of Authority

In descending order:

1. Deployed contract behavior and protocol/circuit invariants.
2. Compatibility and end-to-end tests.
3. Durable event and snapshot schemas already used by production nodes.
4. [`flow-trace/`](flow-trace/) and [`CRATES_ARCHITECTURE.md`](CRATES_ARCHITECTURE.md).
5. This target document.

A cleanup must never silently change committee ordering, threshold meaning, proof multiplicity,
hashing, signatures, circuit witness shape, event identity, or replay semantics. Such a change is a
protocol migration and requires explicit versioning and compatibility tests.

## Canonical Module Structure (Normative)

The directory names below are architectural vocabulary, not suggestions. Code with the same role
must use the same name in every crate. Do not create parallel synonyms such as `services/`,
`business/`, `core/`, `manager_logic/`, or `helpers/` for these responsibilities.

```text
crates/<crate>/src/
  lib.rs        module declarations, public re-exports, and composition only
  domain/       protocol values, invariants, calculations, deterministic validation
  workflow/     workflow state, accepted inputs, transitions, and effect intents
  actors/       Actix mailbox ownership, scheduling, supervision, and dispatch
  adapters/     concrete storage, FHE/ZK, EVM, libp2p, clock, and process I/O
  runtime/      startup/composition coordinators that are not actors, when needed
  messages.rs   stable crate-owned message vocabulary, when needed
  repo.rs       repository ports, keys, and factories, when needed
```

Every actor-bearing protocol crate uses `domain/`, `workflow/`, and `actors/` when it has those
responsibilities. An adapter crate such as `e3-evm` may legitimately have no workflow; a leaf math
crate has no reason to create empty actor or adapter directories. Omitting an inapplicable layer is
allowed. Renaming a layer or placing its responsibility in another layer is not.

Business logic has exactly two homes:

- `domain/` for rules and calculations that do not know which workflow stage invoked them;
- `workflow/` for deterministic progression across stages and the intents that progression emits.

It does not live in `actors/`, `adapters/`, `lib.rs`, or an unclassified support file.

### Actor package template

A cohesive actor that fits in one production file may use `actors/<actor>.rs`. Once it needs a
second responsibility or approaches the 300-line review threshold, it becomes a directory with
this shape:

```text
actors/<actor>/
  mod.rs          actor struct, owned runtime state, parameters, construction, public surface
  handlers.rs     Handler/Actor implementations, or handlers/ when grouped by message family
  runtime/        correlation, bounded dispatch, persistence, timers, and concrete effect calls
  tests.rs        focused actor tests, or tests/ when several test concerns exist
```

`mod.rs`, `handlers`, `runtime`, and `tests` are the only top-level categories in a complex actor
package. Protocol names such as `c4`, `dkg`, `completion`, `rebroadcast`, or `transactions` belong
under `runtime/`; message-family names belong under `handlers/`. This gives every actor the same
entry points while still allowing protocol-specific vocabulary one level lower.

`handlers` may validate the outer envelope, call a workflow transition, commit the result, and
dispatch returned intents. `runtime` may translate intents into existing bus/worker calls. Neither
category owns threshold rules, canonical ordering, proof validity, or state-machine legality.

### Workflow package template

```text
workflow/<workflow>/
  mod.rs          public workflow surface and module wiring
  state.rs        durable/derivable workflow state and accepted input types
  transitions.rs  deterministic reducer, or transitions/ when grouped by input/phase
  intents.rs      typed effect intents when they are not declared beside the transition
  validation.rs   workflow-specific pure validation, when needed
  tests.rs        actor-free transition tests, or tests/ when several concerns exist
```

Phase names may occur below `transitions/`, never as unexplained peers of `state.rs`. A workflow
must not import from `crate::actors`, and a domain module must not import from either `workflow` or
`actors`.

Dependencies point inward:

```text
actors/runtime ───► workflow ───► domain
      │                 │
      └──────────► adapter ports
                        ▲
                  concrete adapters
```

- `domain` must not depend on Actix, Tokio, repositories, network clients, wall-clock calls, or
  process execution.
- `workflow` must not call concrete I/O. It returns explicit intents.
- `actors` and `runtime` may depend on the workflow API and adapter ports, but must not implement
  cryptographic or protocol calculations inline.
- `adapters` implement effect ports and translate external data at trust boundaries. They do not
  decide protocol progression.

Cross-crate dependencies must follow the workspace layering documented in
[`CRATES_ARCHITECTURE.md`](CRATES_ARCHITECTURE.md). A lower-level crate must not import an
actor-bearing orchestration crate merely to reuse a value type.

## Domain Layer

The domain layer owns rules whose result depends only on its inputs, including:

- threshold and committee calculations;
- canonical party, score, proof, and ciphertext ordering;
- membership and multiplicity validation;
- commitment/link consistency rules;
- state-machine legality;
- stable IDs, hashes, and signature preimages;
- typed decisions such as `Accept`, `IgnoreDuplicate`, `Reject`, `FailE3`, or `Accuse`.

Domain functions return typed errors and decisions. They do not publish events, log as their only
error handling, read the clock, or mutate actor state.

Protocol-specific invariants must be named and tested. Important examples include:

- runtime `party_id` is derived from the finalized committee normalized by ascending ticket score;
- the active aggregator is the lowest non-expelled `party_id`;
- the DKG aggregation circuit receives exactly `H` canonical honest NodeFold proofs and exactly `N`
  ordered committee addresses;
- C2a/C2b are singleton proofs, while C3a/C3b follow the configured recipient/row multiplicities;
- TrBFV and Noir witness dimensions come from the active preset, never from incidental vector size.

## Workflow Layer

Long-running protocol behavior is modeled as a deterministic transition:

```rust,ignore
pub struct Transition<S, I> {
    pub state: S,
    pub intents: Vec<I>,
}

pub trait Workflow {
    type State;
    type Input;
    type Intent;
    type Error;

    fn reduce(
        state: &Self::State,
        input: Self::Input,
    ) -> Result<Transition<Self::State, Self::Intent>, Self::Error>;
}
```

An input is a fact already accepted at a trust boundary: a chain observation, verified peer message,
timer firing, effect result, or operator command. An intent describes work to perform, for example:

```rust,ignore
enum DkgIntent {
    PersistDeadline { deadline: Timestamp },
    GenerateEncryptionKey { operation_id: OperationId },
    VerifyShareBundle { operation_id: OperationId, party_id: PartyId },
    BroadcastThresholdShare { operation_id: OperationId },
    PublishFailure { operation_id: OperationId, reason: FailureReason },
}
```

Reducers may update persisted workflow state and emit zero or more intents. They do not await,
spawn, address actors, or execute an intent themselves.

Each intent that can change protocol outcome has:

- a stable operation/idempotency key derived from E3, stage, party, artifact type, and index;
- enough versioned data to retry after restart;
- an explicit result type;
- a retry classification (`never`, `bounded`, `until deadline`, or `operator intervention`);
- a terminal failure mapping where retry cannot restore progress.

## Actor Layer

An actor owns serialized access to one runtime partition. It may:

- receive and authenticate messages;
- preserve per-E3 ordering;
- load and persist workflow state;
- call a reducer;
- durably record transitions and intents;
- dispatch committed intents to effect runners;
- apply correlated results;
- schedule persisted deadlines;
- cancel work and stop child actors;
- expose health, queue, and progress signals;
- supervise or recreate owned children.

An actor handler must not:

- perform BFV/TrBFV/Noir calculations;
- execute `bb`, EVM RPC, libp2p, filesystem, or repository operations inline;
- encode protocol validity as an unstructured sequence of mutations and `do_send` calls;
- ignore a full or closed correctness-critical mailbox;
- keep restart-critical progress solely in memory;
- use detached tasks with no owner, cancellation, or shutdown barrier.

“Thin” is about responsibility, not line count. A handler that validates an envelope, invokes one
transition, commits it, and dispatches its intents is thin even if the surrounding actor contains
careful runtime plumbing. A 20-line handler that performs an irreversible unacknowledged send is not
architecturally sound.

## Adapter and Effect Layer

Effect runners own concrete side effects:

- BFV/TrBFV computation;
- ZK proving and verification processes;
- EventStore and snapshot storage;
- EVM reads and transaction submission;
- libp2p publication and synchronization;
- clocks and timers;
- local secret encryption and filesystem access.

Heavy computation runs in bounded worker pools, never in an actor mailbox thread. Pools are bounded
by both job count and estimated bytes. Jobs report correlation ID, operation ID, success/failure,
timing, and cancellation outcome.

Adapters translate external representations into domain values and enforce the boundary's trust
policy. Peer identity, committee membership, claimed party slot, signature, chain ID, E3 ID, proof
type, payload size, and schema version are checked before a message can drive a workflow.

## Durable Processing Model

The target event path is:

```text
EVM / libp2p / operator
          │
          ▼
 authenticated ingress adapters
          │
          ▼
 durable journal + stable event identity
          │
          ▼ partition by (chain_id, e3_id)
 per-E3 workflow actor → pure reducer
          │
          ▼
 committed effect intents
          │
          ▼
 bounded effect runners
          │
          ▼
 durable, correlated results ─────► workflow actor
```

Delivery is at-least-once. Exactly-once execution is not assumed. Correctness comes from stable
identity, idempotent transitions, effect deduplication, and on-chain/read-before-write guards.

For every correctness-relevant step:

1. Validate and deduplicate the input.
2. Reduce it to a new state plus intents.
3. Atomically commit the state transition and/or an outbox record before dispatch.
4. Acknowledge the input only after that commit succeeds.
5. Execute intents outside the actor's critical section.
6. Persist the correlated result before it can unlock the next transition.
7. Retry according to policy until success, terminal failure, cancellation, or persisted deadline.

If the current storage abstraction cannot atomically commit a snapshot and outbox entry, record the
intent itself as the durable source of truth and make replay re-derive the state. Never mutate memory
and then rely on a fire-and-forget persistence message as proof of durability.

## State Classification

Every state field is classified during review:

| Class | Meaning | Requirement |
| --- | --- | --- |
| Durable | Losing it can change outcome or stall progress | Persist with a versioned schema before acknowledgement |
| Derivable | Reconstructible from authoritative durable facts | Document the source and test reconstruction |
| Ephemeral | Cache/telemetry only; safe to lose | Bound it and make loss behavior explicit |

Pending proof bundles, decrypted-share progress, accusation votes/timeouts, retry state, active
aggregator designation, deadlines, and undispatched external effects are durable unless a stronger
authoritative source can deterministically recreate them.

Snapshots are optimization checkpoints, not a second authority. Event replay from an earlier
checkpoint and snapshot hydration at the same logical point must produce equivalent workflow state
and pending intents.

## Event and Message Taxonomy

Names must reflect semantics. Persisting all messages under one `Event` label hides important
reliability differences.

| Kind | Meaning | Examples | Default durability |
| --- | --- | --- | --- |
| Fact | Something already happened | `CommitteeFinalized`, `ProofVerified`, `CiphertextPublished` | Durable |
| Intent | Work that must be attempted | `GenerateC5`, `PublishCommittee`, `ScheduleDeadline` | Durable outbox |
| Result | Outcome of an intent | `C5Generated`, `PublishConfirmed`, `ComputeFailed` | Durable |
| Query | Request for current information | health/status/repository lookup | Ephemeral |
| Infrastructure signal | Runtime lifecycle | `EffectsEnabled`, readiness, shutdown | Durable only when needed for recovery/audit |

Legacy names may be retained for wire/schema compatibility, but their semantic kind must be
documented. For example, `ComputeRequest` is an intent even if its historical Rust type is stored in
the event journal.

All durable envelopes carry schema version, event ID, causation ID, origin ID, chain ID where
applicable, aggregate/E3 key, source, and timestamp/watermark metadata. Received network events keep
the sender's stable identity; local transport metadata must not accidentally create a new logical
event on every replay.

## Routing, Backpressure, and Ordering

- Protocol work is partitioned by `(chain_id, e3_id)` so one expensive or blocked E3 cannot pause
  unrelated E3s.
- Ordering is guaranteed within a partition. Global total ordering is used only where a documented
  invariant requires it.
- Correctness-critical sends are acknowledged and bounded by timeout. A failed send is retried or
  escalated; it is never only logged.
- Buffers are bounded by both item count and bytes. Overflow has an explicit policy and metric.
- Replay uses bounded paging/merge and the same acknowledgement semantics as live delivery.
- Queries and telemetry may use lossy delivery only when callers can distinguish loss/unavailability.

`do_send` is allowed for best-effort telemetry. It is not allowed for state persistence, workflow
progression, timers, proof/results, cleanup, network publication, or external transaction intents.

## Timers and Deadlines

Persist the absolute protocol deadline and timer purpose, not only an in-memory Actix handle. On
restart, compare the persisted deadline with an injected clock and deterministically emit either a
new timer intent or the overdue input. Timer cancellation is itself part of the workflow transition.

Staggered submitters use a stable rank and persisted attempt state. Restarting must not reset a
fallback delay in a way that suppresses the only remaining submitter.

## Choreography and Local Orchestration

Interfold remains choreographed across nodes and contracts: no node is the global coordinator.
Inside one node, a per-E3 workflow is explicitly orchestrated so its durable state answers:

- what facts have been accepted;
- which stage and canonical participants apply;
- which operations are pending, running, succeeded, or terminally failed;
- which deadlines remain;
- which actor/worker owns each in-flight operation.

The `E3LifecycleCoordinator` is a projection, not the source of truth. Authoritative stage comes
from canonical chain facts plus durable local workflow facts. A projection may be deleted and
rebuilt without changing execution.

## Schema Evolution

Rust type compatibility is not a storage migration strategy. Every durable event, snapshot, and
outbox payload has an explicit schema version and decoder policy.

- Adding/removing/reordering a field requires a compatibility test against checked-in fixtures.
- Bincode payloads are never assumed self-describing or forward compatible.
- A version mismatch either runs a tested migration or fails startup with an actionable error.
- Migrations are restartable and do not destroy the previous data until the replacement is verified.
- Wire compatibility and storage compatibility are reviewed separately.

## Testing Requirements

Domain and workflow tests are actor-free and deterministic. They cover:

- every legal and illegal transition;
- duplicates, reordering, missing parties, expulsion, and threshold boundaries;
- canonical ordering and proof multiplicities for every supported preset;
- intent idempotency keys and terminal failure mapping.

Runtime tests cover:

- mailbox saturation, unavailable recipients, and bounded timeouts;
- worker failure/cancellation and actor restart;
- duplicate fact/result delivery;
- shutdown barriers and effect gating.

Recovery tests use a crash matrix around each effect:

1. before transition commit;
2. after commit but before dispatch;
3. while the effect is running;
4. after external success but before result commit;
5. after result commit but before the next input.

Each case must converge to the same state and external outcome as uninterrupted execution. Snapshot
hydration and full replay must produce equivalent state plus pending intents.

Integration tests assert end-to-end protocol behavior. Long cryptographic tests run after fast
domain, workflow, crate, and workspace checks have passed.

The recursive `node_fold_correlated_sparse_self_slot_proves_and_verifies` test and the full
`test_trbfv_actor` flow belong to the slow lane. Debug builds may spend minutes in real proof/FHE
work and may emit a "running for over 60 seconds" progress warning. That warning is not a failure;
the test harness exit status is authoritative. Keep these tests enabled, run them last, and give CI
an explicit timeout based on measured debug-runtime headroom rather than weakening their coverage.

## Code Shape and Review Heuristics

Files and structs are organized around one reason to change. Roughly 300 lines is a review trigger,
not a mechanical limit: generated bindings, tables, and cohesive algorithms may be longer. A large
file must not combine unrelated message definitions, domain rules, persistence, actor handlers,
effect execution, and tests.

A struct is likely a god object when it owns several independent lifecycles, mixes durable and
ephemeral state without classification, or requires most dependencies for only a few handlers.
Extract behavior by responsibility:

1. pure domain rule or value;
2. workflow state and reducer;
3. effect port/runner;
4. repository/codec;
5. actor façade and message routing;
6. test support.

Do not hide coupling by splitting one `impl` into arbitrary files while keeping the same god struct.
The goal is smaller ownership and explicit contracts, not a lower line count alone.

## Migration Rules

Refactor one recoverable protocol slice at a time:

1. Lock current behavior with protocol and compatibility tests.
2. Classify its messages and state.
3. Extract pure domain decisions.
4. Introduce workflow state, inputs, intents, and stable operation IDs.
5. Add durable dispatch/result handling.
6. Move concrete I/O into bounded adapters.
7. Add crash/replay equivalence tests.
8. Remove the legacy path only after both paths agree.
9. Update `CRATES_ARCHITECTURE.md` and the relevant flow trace in the same change.

Temporary bridges are permitted when named, tested, and tracked for removal. New code must not add
another unacknowledged correctness edge merely because a neighboring legacy path still has one.

## Definition of Done for an Actor Refactor

An actor is considered architecturally thin when:

- its protocol decisions can be tested without Actix;
- its durable state and derivable/ephemeral fields are explicit;
- its handlers translate inputs, invoke reducers, commit, and dispatch intents;
- heavy work and external I/O run behind bounded effect interfaces;
- every critical send and persistence step has success/failure semantics;
- restart restores pending deadlines and effects;
- duplicate and out-of-order delivery is deterministic;
- supervision, cancellation, cleanup, readiness, and metrics are defined;
- documentation and flow traces describe the implemented behavior.

The actor model is therefore retained, but actors cease to be containers for the whole protocol.
They become reliable runtime shells around deterministic, recoverable workflows.
