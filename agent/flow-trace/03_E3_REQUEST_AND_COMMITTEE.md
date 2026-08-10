# Part 3: E3 Request & Committee Formation

## Overview

An E3 (Encrypted Execution Environment) is the core unit of work in the Interfold protocol. A
requester pays a fee, a committee of ciphernodes is selected via sortition, and the committee
collectively generates encryption keys through DKG.

---

## E3 Lifecycle Stages

```
None → Requested → CommitteeFinalized → KeyPublished → CiphertextReady → Complete
                                                                       ↘ Failed
```

Each transition has a deadline. Missing a deadline allows anyone to call `markE3Failed()`.

Governance configures the fee token, its expected decimals, and every raw-unit pricing term through
`setFeeAssetConfig()`. The update is atomic, and the event contains the complete configuration. The
decimals check confirms the unit scale only; it does not prove that two tokens have the same
economic value. Each request snapshots the active token, so later fee-asset changes do not alter an
existing E3's escrow or settlement unit.

---

## Step 1: E3 Request (On-Chain)

**Contract:** `Interfold.sol` → `request(E3RequestParams)`

```
Requester calls: Interfold.request({
  committeeSize: <minimum | micro | small>,
  inputWindow: [start, end], // when inputs are accepted
  e3Program: <address>,      // computation program contract
  e3ProgramParams: <bytes>,  // ABI-encoded program parameters
  computeProviderParams: <bytes>,
  customParams: <bytes>
})
│
├─ VALIDATION:
│   ├─ Resolve the build-generated active crypto configuration.
│   │    The current build is insecure-512 / minimum [H=2, N=3, T=1].
│   │    A different parameter hash, committee shape, or verifier H/T is rejected.
│   ├─ inputWindow[0] >= block.timestamp (start in future)
│   ├─ inputWindow[1] >= inputWindow[0] (end after start)
│   ├─ inputWindow[1] + computeWindow >
│   │    block.timestamp + registry.sortitionSubmissionWindow()
│   │    (committee finalization must be possible before compute expires)
│   ├─ total duration < maxDuration
│   └─ e3Programs[e3Program] == true (program whitelisted)
│
├─ FEE CALCULATION:
│   ├─ fee = getE3Quote()
│   │   → InterfoldPricing uses the active circuit [T, N].
│   │   → The quote uses T for decryption work and H for on-chain viability only.
│   │   → It also uses the time windows,
│   │     proof counts, availability, decryption/publication costs, and margin
│   │   → availability covers at least request time through input-window end
│   │   → a later equal-length input window therefore costs more
│   ├─ feeToken.transferFrom(requester, address(this), fee)
│   └─ e3Payments[e3Id] = fee  (stored per-E3)
│       _e3FeeTokens[e3Id] = feeToken  (survives global token rotation)
│
├─ E3 CREATION:
│   ├─ e3Id = nexte3Id++
│   ├─ Snapshot Interfold dependencies for this E3:
│   │   registry, bonding registry, refund manager, and slashing manager
│   │   → later global rotations apply only to new requests
│   │   → the slashing manager registers this E3's refund destination in
│   │     BondingRegistry for proposal-scoped ticket-slash routes
│   ├─ snapshottedRefundManager.snapshotE3Policy(e3Id, registry)
│   │   → freezes refund/slash allocation, treasury, policy version,
│   │     request-time Interfold, committee registry, bonding registry,
│   │     and slashing manager
│   ├─ seed = uint256(keccak256(block.prevrandao, e3Id))
│   │   → Shared per-E3 ticket-scoring input only; not BFV key material and
│   │     not relied upon for cryptographic unpredictability.
│   │
│   ├─ encryptionSchemeId = e3Program.validate(
│   │     e3Id, seed, e3ProgramParams, computeProviderParams, customParams
│   │   )
│   │   → Program validates params and returns which encryption scheme to use
│   │
│   ├─ decryptionVerifier = decryptionVerifiers[encryptionSchemeId]
│   │   → Must exist (registered by admin for this scheme)
│   │
│   ├─ Store E3 struct:
│   │   e3s[e3Id] = E3 {
│   │     seed, threshold, requestBlock: block.timestamp,  // H-26: EIP-6372 clock
│   │     inputWindow, encryptionSchemeId, e3Program,
│   │     e3ProgramParams, customParams, decryptionVerifier,
│   │     requester: msg.sender
│   │   }
│   │
│   ├─ _e3Requesters[e3Id] = msg.sender
│   └─ _e3Stages[e3Id] = E3Stage.Requested
│
├─ COMMITTEE REQUEST:
│   ├─ ciphernodeRegistry.requestCommittee(e3Id, seed, threshold)
│   │   │
│   │   │  ┌─── CiphernodeRegistryOwnable ──────────────────────┐
│   │   │  │                                                     │
│   │   │  │  requestCommittee(e3Id, seed, threshold) {          │
│   │   │  │    1. require(!committees[e3Id].initialized)        │
│   │   │  │    2. Snapshot request-time Interfold, bonding,     │
│   │   │  │       slashing manager, and fold verifier           │
│   │   │  │       → ask SlashingManager to snapshot its         │
│   │   │  │         bonding, registry, Interfold, refund routes │
│   │   │  │    3. Query eligibilityAt(address(0),               │
│   │   │  │         requestBlock - 1) and require               │
│   │   │  │         threshold[1] <= activeOperatorCount         │
│   │   │  │       → Count and submissions use one boundary      │
│   │   │  │    4. committees[e3Id] = Committee {                │
│   │   │  │         initialized: true,                          │
│   │   │  │         seed: seed,                                 │
│   │   │  │         requestBlock: block.timestamp, // H-26      │
│   │   │  │         committeeDeadline:                          │
│   │   │  │           block.timestamp + sortitionWindow,        │
│   │   │  │         threshold: threshold                        │
│   │   │  │       }                                             │
│   │   │  │    5. sortitionTicketPrices[e3Id] =                 │
│   │   │  │         bondingRegistry.ticketPrice()               │
│   │   │  │       → Freeze ticket capacity for this E3          │
│   │   │  │    6. roots[e3Id] = ciphernodes._root()             │
│   │   │  │       → SNAPSHOT the IMT root at this moment        │
│   │   │  │       → Only nodes in tree at request time eligible │
│   │   │  │    7. Emit DkgFoldAttestationContextEstablished(    │
│   │   │  │              e3Id, registry, foldVerifier)          │
│   │   │  │       Emit CommitteeRequested(e3Id, seed, threshold,│
│   │   │  │              requestBlock, committeeDeadline,       │
│   │   │  │              ticketPrice)                           │
│   │   │  │       BondingRegistry records this request-time      │
│   │   │  │       registry as the E3's obligation owner          │
│   │   │  │  }                                                  │
│   │   │  └─────────────────────────────────────────────────────┘
│   │
│   └─ Set deadlines:
│       _e3Deadlines[e3Id].computeDeadline =
│         inputWindow[1] + _timeoutConfig.computeWindow
│
├─ EMIT: E3Requested(e3Id, e3, e3Program)  // seed & params inside E3 struct
├─ EMIT: E3StageChanged(e3Id, E3Stage.None, E3Stage.Requested)
│
└─ RETURN: (e3Id, e3)
```

---

## Step 2: Sortition — Committee Selection (Rust-Side)

When the running ciphernodes detect `DkgFoldAttestationContextEstablished`, `E3Requested`, and
`CommitteeRequested` events from the chain:

At startup, each ciphernode loads the saved request-time registry and verifier for every active E3.
It gives this data to the proof actors and registry writers before event replay starts. Events after
the latest snapshot then replay in order and add any newer E3 contexts.

### 2a. Request Event Processing

```text
CiphernodeRegistrySolReader decodes DkgFoldAttestationContextEstablished
│
└─ Stores the E3's request-time registry and verifier for signing, validation, and publication

InterfoldSolReader decodes IInterfold::E3Requested log
│
├─ If the ABI log is well-formed but its committee-size or BFV-preset enum is newer than this
│  binary supports, records the provider log as internally processed and skips participation;
│  historical ordering advances, while malformed ABI data still fails chain ingestion closed
│
├─ Publishes InterfoldEvent::E3Requested {
│     e3_id, threshold_m, threshold_n,
│     seed, params, error_size, esi_per_ct
│   }
│
├─ FheExtension.on_event():
│   └─ Creates Fhe instance from BFV params
│   └─ Stores as dependency in E3Context
│
├─ PublicKeyAggregatorExtension.on_event():
│   └─ Spins up the per-E3 public-key aggregation pipeline
│   └─ KeyshareCreatedFilterBuffer buffers until this node becomes the active aggregator
│
└─ Sortition actor receives E3Requested:
    │
    ├─ Loads the request timepoint and frozen ticket price from CommitteeRequested
    ├─ Calculates buffer = calculate_buffer_size(M, N)
    │
    ├─ ScoreBackend.get_committee():
    │   │
    │   ├─ Loads nodes from NodeStateStore at requestBlock - 1
    │   │   (filter: active at that time, historical tickets > 0)
    │   │   → Every node uses the complete request-time ticket range
    │   │   → Local and remote active-job counts do not change this range
    │   │
    │   ├─ For EACH eligible node:
    │   │   For EACH ticket t in [1..availableTickets]:
    │   │     score = keccak256(address || t || e3Id || seed)
    │   │     → Deterministic score per (node, ticket, e3)
    │   │
    │   ├─ Per node: keep only the LOWEST scoring ticket
    │   │   (each node's best chance)
    │   │
    │   ├─ Sort ALL nodes by their best score (ascending)
    │   │
    │   └─ Select top N nodes (lowest scores win)
    │       → Returns committee list with party indices
    │
    └─ Sends WithSortitionTicket<E3Requested> to CiphernodeSelector
        │
        ├─ If THIS node is in the selected committee:
        │   ├─ Check only this node's voluntary active-job limit
        │   ├─ If capacity remains:
        │   │   ticket_id = Some(TicketId::Score(best_ticket_number))
        │   │   party_index = Some(index_in_committee)
        │   └─ If capacity is exhausted: ticket_id = None
        │
        └─ If NOT selected: ticket_id = None
```

### 2b. CiphernodeSelector Processing

```
CiphernodeSelector receives WithSortitionTicket<E3Requested>
│
├─ If ticket_id is Some (this node was selected):
│   ├─ Caches E3Meta { e3_id, threshold_m, threshold_n, seed, ... }
│   ├─ Publishes TicketGenerated {
│   │     e3_id,
│   │     ticket_id: TicketId::Score(ticket_number),
│   │     party_index: index_in_local_score_ranking
│   │   }
│   └─ This event triggers on-chain ticket submission
│
└─ If ticket_id is None:
    └─ Does nothing (not selected for this E3)
```

### 2c. On-Chain Ticket Submission

```
CiphernodeRegistrySolWriter receives TicketGenerated event
│
└─ Calls contract.submitTicket(e3Id, ticketNumber).send()
    │
    │  ┌─── ON-CHAIN (CiphernodeRegistryOwnable) ──────────────┐
    │  │                                                         │
    │  │  submitTicket(e3Id, ticketNumber) {                     │
    │  │    1. require(committees[e3Id].initialized)             │
    │  │    2. require(!committees[e3Id].finalized)              │
    │  │    3. require(block.timestamp <= committeeDeadline)     │
    │  │    4. require(!submitted[msg.sender])                   │
    │  │       → Each node submits only once                     │
    │  │    5. require(isEnabled(msg.sender) AND                 │
    │  │               _bondingFor(e3Id).isActive(msg.sender) AND│
    │  │               activeAtRequest)                          │
    │  │       → Uses the request-time bonding registry          │
    │  │       → Historical eligibility is the selection rule    │
    │  │       → Current activity is an extra liveness check      │
    │  │                                                         │
    │  │    6. _validateNodeEligibility(e3Id, msg.sender,        │
    │  │                                ticketNumber):           │
    │  │       availableTickets =                                │
    │  │         _bondingFor(e3Id).ticketToken().getPastVotes(   │
    │  │           msg.sender, requestBlock - 1                  │
    │  │         ) / sortitionTicketPrices[e3Id]                 │
    │  │       → Uses the timepoint before the request            │
    │  │       → Uses the request-time ticket price               │
    │  │       → Prevents same-block manipulation                │
    │  │       require(ticketNumber >= 1)                        │
    │  │       require(ticketNumber <= availableTickets)          │
    │  │                                                         │
    │  │    7. score = uint256(keccak256(                        │
    │  │         msg.sender, ticketNumber, e3Id, seed            │
    │  │       ))                                                │
    │  │       → SAME formula as Rust-side computation           │
    │  │       → Both sides agree on scores                      │
    │  │                                                         │
    │  │    8. submitted[msg.sender] = true                      │
    │  │       scoreOf[msg.sender] = score                       │
    │  │                                                         │
    │  │    9. _insertTopN(e3Id, msg.sender, score):             │
    │  │       Maintains array of N lowest-scoring nodes:        │
    │  │       - If < N nodes: just insert                       │
    │  │       - If N nodes: replace highest if new score lower  │
    │  │       - O(N) linear scan per insertion                  │
    │  │                                                         │
    │  │   10. Emit TicketSubmitted(e3Id, msg.sender, score)     │
    │  │  }                                                      │
    │  └─────────────────────────────────────────────────────────┘
```

---

## Step 3: Committee Finalization

### 3a. Deadline Timer (Rust-Side, Committee Members)

```
CommitteeFinalizer actor receives CommitteeRequested event
│
├─ Stores the request during replay and waits until ALL of:
│   ├─ local TicketGenerated.party_index is known
│   └─ EffectsEnabled has fired
│
├─ Calculates wait time:
│   wait = max(committeeDeadline - currentTimestamp, 0)
│          + 1 second
│          + party_index * 5 seconds
│
├─ Schedules a staggered timer
│
├─ When timer fires:
│   └─ Publishes CommitteeFinalizeRequested { e3_id }
│
└─ On E3Failed / E3RequestComplete / E3StageChanged(Complete|Failed):
    └─ Cancels pending timer for this e3_id (if any)
        → Prevents stale finalization attempt after E3 is already terminal
```

### 3b. On-Chain Finalization

```
CiphernodeRegistrySolWriter receives CommitteeFinalizeRequested
│
└─ Calls contract.finalizeCommittee(e3Id).send()
    │
    │  ┌─── ON-CHAIN (CiphernodeRegistryOwnable) ──────────────┐
    │  │                                                         │
    │  │  finalizeCommittee(e3Id) {                              │
    │  │    1. require(initialized && !finalized)                │
    │  │    2. require(block.timestamp >= committeeDeadline)     │
    │  │       → Submission window must have closed (>= not >)  │
    │  │                                                         │
    │  │    3. if topNodes.length < threshold[1]:                │
    │  │       → NOT ENOUGH NODES submitted tickets              │
    │  │       committees[e3Id].failed = true                    │
    │  │       interfold.onE3Failed(e3Id,                          │
    │  │         FailureReason.InsufficientCommitteeMembers)     │
    │  │       Emit CommitteeFormationFailed(e3Id)               │
    │  │       RETURN                                            │
    │  │                                                         │
    │  │    4. SUCCESS PATH:                                     │
    │  │       Copy topNodes → committee (ordered by index)      │
    │  │       For each node in committee:                       │
    │  │         active[node] = true                             │
    │  │       activeCount = committee.length                    │
    │  │       finalized = true                                  │
    │  │                                                         │
    │  │    5. Record one unresolved collateral obligation        │
    │  │       for each finalized member in BondingRegistry       │
    │  │                                                         │
    │  │    6. interfold.onCommitteeFinalized(e3Id)                │
    │  │       │                                                 │
    │  │       │  ┌─ Interfold.sol ────────────────────────────┐  │
    │  │       │  │  onCommitteeFinalized(e3Id) {            │  │
    │  │       │  │    require(stage == Requested)            │  │
    │  │       │  │    stage = CommitteeFinalized             │  │
    │  │       │  │    dkgDeadline = now + dkgWindow          │  │
    │  │       │  │    snapshot each member's reward          │  │
    │  │       │  │      recipient in E3RefundManager         │  │
    │  │       │  │    Emit E3StageChanged(e3Id,              │  │
    │  │       │  │          CommitteeFinalized)              │  │
    │  │       │  │  }                                       │  │
    │  │       │  └──────────────────────────────────────────┘  │
    │  │                                                         │
    │  │    7. Emit SortitionCommitteeFinalized(                 │
    │  │         e3Id, committee, scores                         │
    │  │       )                                                 │
    │  │       [ICiphernodeRegistry event]                       │
    │  │  }                                                      │
    │  └─────────────────────────────────────────────────────────┘
```

Ticket submission changes only the provisional `topNodes` set. Successful finalization grants
membership and `Active` status to the final address-sorted members. Failed formation grants neither.
Finalization also freezes each member's current bond owner as its reward recipient for this E3.
Later bond-owner transfers apply to later committees, not to payments earned by this committee. It
also locks each member's queued collateral against withdrawal. Once Interfold reports `Complete` or
`Failed`, anyone can call `releaseCommittee(e3Id)` on the request-time registry to release all
member obligations atomically.

### 3c. SortitionCommitteeFinalized Event Processing (Rust-Side)

```text
CiphernodeRegistrySolReader decodes SortitionCommitteeFinalized
│  [ICiphernodeRegistry event]
│
├─ Publishes InterfoldEvent::CommitteeFinalized {
│     e3_id, committee: [addr1, addr2, ..., addrN], scores: [s1, s2, ..., sN], chain_id
│   }
│
├─ Sortition actor:
│   └─ Stores finalized committee as a `Committee` struct in persistent map
│       → Provides O(1) address→party_id lookup for later expulsion handling
│       → `CommitteeFinalized` is normalized into ascending address order before storage
│
├─ CiphernodeSelector:
│   ├─ Checks if this node's address is in the committee list
│   ├─ If YES:
│   │   party_id = index of this node in committee array
│   │   Publishes CiphernodeSelected {
│   │     e3_id, threshold_m, threshold_n,
│   │     seed, party_id, ...all E3 metadata
│   │   }
│   │   Publishes AggregatorChanged {
│   │     e3_id,
│   │     is_aggregator = (my node has the lowest non-expelled party_id in the
│   │                      address-sorted finalized committee)
│   │   }
│   └─ If NO: does nothing for this E3
│
└─ KeyshareCreatedFilterBuffer:
    └─ Stores committee set
    └─ Keeps buffering until AggregatorChanged(is_aggregator=true)
    └─ Then flushes buffered KeyshareCreated events from verified committee members
```

---

## Timeline & Deadlines

```
Time ──────────────────────────────────────────────────────────►

│ request()      │ sortitionWindow │ dkgWindow     │
│                │                 │               │
│ E3Requested    │ CommitteeDeadline│ DKG Deadline  │
│ CommitteeReq.  │                 │               │
│                │ Ciphernodes     │ Must complete  │
│                │ submit tickets  │ DKG by here    │
│                │                 │               │
│                │ finalizeComm.() │               │
│                │ CommFinalized   │               │
│                │ ───►DKG starts  │               │

If a stage deadline is missed → anyone can call `markE3Failed()`.
The registry must finalize a ready committee.
```

---

## Key Design Properties

1. **Deterministic sortition**: Both Rust and Solidity compute
   `keccak256(address, ticket, e3Id, seed)`. The on-chain contract verifies what the off-chain node
   computed.

2. **Snapshot-based eligibility**: The eligible count, operator eligibility, and ticket balances use
   `requestBlock - 1`. The ticket price is frozen in the request transaction. Rust and Solidity
   consume those same values, so later activation, collateral, or price changes cannot alter the
   candidate set. All nodes compute the same buffered winner set. A selected node can decline its
   own submission when its local active-job capacity is exhausted.

3. **Runtime committee order**: both the on-chain registry and Rust runtime normalize the finalized
   committee into ascending address order before deriving `party_id`. This keeps party IDs,
   aggregator failover, proof inputs, and `CommitteeHashLib.hash(topNodes)` aligned.

4. **Active aggregator selection**: `CiphernodeSelector` derives `AggregatorChanged` from the
   finalized committee plus enriched `CommitteeMemberExpelled` events. The active aggregator is the
   lowest non-expelled `party_id` in the address-sorted runtime committee.

5. **Permissionless finalization**: Anyone can call `finalizeCommittee()` after the deadline — no
   single point of failure.

6. **IMT root snapshot**: The Merkle tree root is captured at request time. Nodes that join/leave
   after the request don't affect this E3's committee.

7. **Dependency graph snapshot**: Each E3 drains through its request-time registry, bonding,
   slashing, refund, and Interfold relationships. Admin rotation changes defaults for later E3s but
   cannot redirect or brick committee callbacks, proof checks, failure settlement, rewards, or
   slashed-fund routing for an in-flight E3. A request atomically records the complete graph before
   committee formation begins. Because applying a new graph requires several governance
   transactions, request-time validation rejects every intermediate state; a requester can only
   freeze the fully old or fully new graph.

8. **Committee collateral follows the E3**: The request-time registry owns the E3's collateral
   obligations. Successful finalization locks every member. A later registry rotation cannot open,
   release, or strand those obligations through the replacement registry.

9. **Operator identity is unchanged by delegated bonding**: tFOLD is minted to the operator, and
   `submitTicket` is still sent by the operator key. Sortition hashes, eligibility snapshots,
   committee membership, and party IDs never use the bond-owner address.

10. **E3 program bootstrap and governance**: The production deploy requires one deployed E3 program.
    `Interfold.initialize` registers it before it transfers ownership to the Safe. Every
    registration rejects an address without runtime code. After initialization, only the owner can
    append another program.

---

## Cluster 7 audit additions (post-fix semantics)

### H-04 — snapshot-based eligibility

`CiphernodeRegistryOwnable._validateNodeEligibility` derives the per-node ticket weight from the
`InterfoldTicketToken` ERC20Votes checkpoint history at `committee.requestBlock - 1` (EIP-6372
timestamp clock). Same-block or post-request rebalancing therefore cannot inflate a node's selection
weight. `submitTicket` also checks historical eligibility and the current `isActive` flag in the
request-time bonding registry.

### M-28 — immutable per-E3 sortition state

At request timestamp `T`, `BondingRegistry.eligibilityAt` supplies the active count and individual
eligibility from `T-1`. `CiphernodeRegistryOwnable` freezes the current ticket price and emits it in
`CommitteeRequested`. The Rust sortition actor stores that event, reads activity and balances from
`T-1`, and uses the frozen price. Solidity checks the same historical state and price during ticket
submission. Current registry membership and activity remain additional liveness checks.

An upgrade with committees still in the `Requested` stage must backfill
`sortitionTicketPrices[e3Id]` with each E3's request-time price before ticket submission resumes. A
zero value makes every submission revert with `InvalidTicketNumber()`. Terminal E3s and committees
requested after the upgrade need no backfill.

### M-33 — `markE3Failed` grace period

When `markFailedGracePeriod > 0` (set via `Interfold.setMarkFailedGracePeriod`), calling
`markE3Failed` within `deadline … deadline + markFailedGracePeriod` is restricted to
`{ original requester, contract owner, active finalized committee member }`. After that window, any
caller can finalize the failure. The default value of `0` preserves the permissionless flow.

### H-26 — timestamp-clock `requestBlock`

`Committee.requestBlock` stores `block.timestamp` (EIP-6372 timestamp mode) so that `getPastVotes`
lookups against the `InterfoldTicketToken` resolve consistently across L1 and L2 clocks. The field
name is preserved for storage and event ABI compatibility.

### Committee observability events

The EVM reader has typed coverage for `CommitteeFormationFailed`, `CommitteeActivationChanged`, and
`CommitteeViabilityUpdated` in addition to ticket submission, finalization, publication, and
expulsion. These facts are stored in the E3's chain aggregate and projected into the dashboard's
committee stage, including submitted/required thresholds and post-expulsion viability.
