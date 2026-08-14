# Part 2: Tokens, Bonding & Activation

## Overview

Before a node can register, it must stake two types of collateral:

1. **FOLD tokens** (license bond) — governance/utility token, staked directly
2. **Collateral via tFOLD tickets** (ticket collateral) — the configured ERC-20 asset is wrapped
   into non-transferable InterfoldTicketToken. The planned launch asset is sUSDS.

Collateral ownership and operator identity are separate namespaces:

- `operator` is the hot node key and remains the registry, ticket, sortition, DKG, ban, and slash
  identity.
- `bondOwnerOf(operator)` is the wallet that funds and controls collateral. The operator must set it
  to a nonzero address before any position action. It may choose itself, although a separate cold
  wallet or Safe is recommended. The current owner can later rotate ownership through a two-step
  proposal and acceptance, provided removing the position's FOLD credit does not break the old
  owner's locked-balance coverage.
- Positions use only the owner-authorized `...For(operator)` calls. Ticket tokens are minted to the
  operator; exit payouts go only to the owner.
- A bond owner may fund multiple operator keys. `totalBonded(owner)` aggregates its active and
  pending FOLD across those keys so FOLD wallet-level lock accounting remains correct.

---

## Token Architecture

```
┌───────────────────────────────────────────────────────────┐
│                    InterfoldToken (FOLD)                     │
│  ERC20 + ERC20Permit + ERC20Votes + AccessControl         │
│  + Ownable2Step                                            │
│                                                           │
│  MAX_SUPPLY: 1,200,000,000 (1.2B with 18 decimals)       │
│  Immutables: CCA_START, CCA_END, CLAIM_SOURCE,            │
│              BONDING_REGISTRY (set at construction)        │
│                                                           │
│  Lifecycle phases (derived from CCA window + TGE):        │
│    Virtual → PublicSale → Cooldown → Live                 │
│    - Virtual: pre-sale setup                                │
│    - PublicSale: CCA bidding window                        │
│    - Cooldown: CCA ended, TGE not yet called               │
│    - Live: TGE fired (permissionless after cooldown)       │
│                                                           │
│  Minting (all pre-TGE phases):                            │
│    - mint(recipient, amount, label)                        │
│      DEFAULT_ADMIN_ROLE — unlocked tokens                  │
│    - mintAllocations(MintAllocation[])                     │
│      MINTER_ROLE — tokens locked under a policy            │
│                                                           │
│  Pre-TGE transfer gate (phase-based, automatic):          │
│    Allowed: bonding registry, claim source, whitelisted    │
│    Blocked: all other transfers                            │
│    Once TGE fires, all transfers unrestricted              │
│                                                           │
│  Lock system (wallet-level pooled enforcement):           │
│    - createLockPolicy(id, LockPolicy) → write-once         │
│      LOCK_MANAGER_ROLE                                     │
│    - linkClaim(account, amount, policyId)                  │
│      LOCK_MANAGER_ROLE                                     │
│    - LockPolicy: { holdUntil, Curve { anchor, start,      │
│        cliffDuration, vestDuration } }                     │
│    - Anchor: Absolute (fixed start) | Tge (tgeTimestamp)   │
│    - PENDING_LOCK_POLICY_ID for unclassified claims        │
│    - Queued locks consumed by later claims (linkClaim)     │
│                                                           │
│  Lock invariant for transfers:                             │
│    transferable = balance - max(0, lockedBalance -         │
│      BONDING_REGISTRY.totalBonded(account))                │
│    Transfer reverts with InsufficientUnlockedBalance       │
│    if value > transferable                                 │
│                                                            │
│  Lock sunset (NO_MORE_LOCKS, immutable):                   │
│    - Absolute timestamp set at deployment                  │
│    - createLockPolicy rejects any policy that could        │
│      outlast the sunset (curves and holdUntil)             │
│    - From NO_MORE_LOCKS on, _update skips all lock         │
│      accounting (vanilla ERC20); PENDING locks die too     │
│                                                            │
│  Whitelisting:                                             │
│    - setTransferWhitelisted(addr, bool)                    │
│      WHITELIST_ROLE — pre-TGE transfer gate                │
│    - setLockWhitelisted(addr, bool)                        │
│      LOCK_MANAGER_ROLE — exempt from claim-source locks    │
│                                                           │
│  Used as: LICENSE BOND token                              │
└───────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────┐
│              InterfoldTicketToken (tFOLD)                      │
│  ERC20Wrapper over the configured collateral asset        │
│                                                           │
│  NON-TRANSFERABLE: _update() reverts on transfer          │
│  NO DELEGATION: delegate() reverts                        │
│  NO APPROVALS: approve() reverts                          │
│                                                           │
│  Only BondingRegistry (registry role) can:                │
│    depositFor()  → wrap collateral, mint tFOLD to operator │
│    depositFrom() → pull collateral, mint tFOLD to operator │
│    burnTickets() → burn tFOLD, NO underlying returned       │
│    withdrawTo()  → burn tFOLD, return collateral           │
│    payout()      → send underlying from payableBalance    │
│                                                           │
│  Used as: TICKET COLLATERAL token                         │
└───────────────────────────────────────────────────────────┘
```

`setBondingAssetConfig()` updates both tokens, their expected decimals, `ticketPrice`, and
`licenseRequiredBond` in one transaction. Decimal checks confirm the raw-unit scale but do not
establish either token's economic value. Both assets must transfer exact amounts and must not rebase
account balances. Deposits verify the custody increase. Exits, payouts, and license transfers verify
the recipient increase and custody decrease. A mismatch reverts the complete transaction.

The protocol deployment configuration keeps `feeToken` separate from `ticketUnderlyingToken`. The
planned launch uses USDS for fees and sUSDS for ticket collateral. A fixed `ticketPrice` or ticket
slash penalty represents a fixed number of sUSDS shares, not a fixed USDS redemption value.

Bonding-asset rotation is liability-gated. A replacement ticket wrapper cannot be configured while
the old wrapper has issued tickets or a payable balance. The registry tracks `totalLicenseLiability`
across active FOLD bonds, queued exits, and slashed funds; it decreases only when a claim or
treasury withdrawal actually consumes an obligation. Unsolicited old-token dust is therefore
distinguishable from operator liabilities. `setBondingAssetConfig()` sends that surplus to
`slashedFundsTreasury` before it validates and applies the replacement in the same transaction, so a
new donation cannot interleave and block rotation. `sweepLicenseSurplus()` remains available for
standalone cleanup. Rotation also waits for every E3 assignment, slash lock, and pending slash route
to close. Replacement assets must be deployed contracts; the only zero exception is the one-time
license-token placeholder used to resolve the circular FOLD/BondingRegistry deployment.

The exit delay must exceed the current sortition submission window and every unexpired request-time
deadline. Each request raises a monotonic deadline watermark. `exitDelayFloor()` returns the larger
of the current window and the remaining watermark duration. Both contracts check this floor when
governance changes either value or connects a replacement registry. BondingRegistry rejects a zero
registry pointer. Equality is rejected because a snapshot-weighted ticket remains valid at the
deadline.

---

## Step 1: Bond License

The owner wallet or Safe approves FOLD and calls `bondLicenseFor(operator, amount)`. The registry
pulls from the owner, credits the operator's license position, and credits `totalBonded(owner)`.

```
Bond owner submits bondLicenseFor(operator, 50000)
│
├─ 1. Approve FOLD spend:
│     └─ InterfoldToken.approve(bondingRegistry, 50000)
│        → Allows BondingRegistry to pull FOLD tokens
│
├─ 2. BondingRegistry.bondLicenseFor(operator, 50000)
│     │
│     │  ┌─── ON-CHAIN (BondingRegistry.sol) ──────────────────┐
│     │  │                                                      │
│     │  │  bondLicenseFor(address operator, uint256 amount) {  │
│     │  │    1. require(msg.sender == bondOwnerOf(operator))   │
│     │  │    2. require(amount > 0)                            │
│     │  │    3. operators[operator].licenseBond += amount      │
│     │  │       → Resolve bondOwnerOf(operator) locally         │
│     │  │       → totalBonded(bondOwner) now includes amount   │
│     │  │    4. licenseToken.safeTransferFrom(                 │
│     │  │         msg.sender,   // from bond owner             │
│     │  │         address(this), // to BondingRegistry         │
│     │  │         amount                                       │
│     │  │       )                                              │
│     │  │       → FOLD _update can see the pre-recorded bond   │
│     │  │         and enforce locked-floor accounting          │
│     │  │       → FOLD tokens move from owner → contract       │
│     │  │       → require the registry receives exactly amount │
│     │  │    5. totalLicenseLiability += amount                │
│     │  │    6. _updateOperatorStatus(operator)                │
│     │  │       → May activate if all conditions now met       │
│     │  │    7. Emit LicenseBondUpdated(operator, newBond)     │
│     │  │  }                                                   │
│     │  └──────────────────────────────────────────────────────┘
│     │
└─ Bond is owned by msg.sender and attributed to operator
```

### Locked FOLD bonding

`BondingRegistry.totalBonded(account)` returns FOLD owned by that account across every operator
position it funds, including pending exits that remain slashable/not returned. `InterfoldToken` uses
this view for pooled wallet-level locks, so locked FOLD can be bonded without becoming transferable.
A claim or license slash removes the exact amount from the owner's aggregate credit. Bond-owner
acceptance checks that the previous owner's wallet balance plus its remaining aggregate bond still
covers `lockedBalanceOf(previousOwner)` before migrating a position's credit. Without that check, a
second wallet could claim the migrated bond as unlocked FOLD while the original lock holder remains
empty.

### Activation check after bonding:

```
_updateOperatorStatus(operator):
  wasActive = operators[operator].active

  isNowActive = (
    operators[operator].registered == true
    AND no authorized slashing manager has banned the operator
    AND operators[operator].licenseBond >= ceil(licenseRequiredBond * licenseActiveBps / 10000)
        // Default: licenseActiveBps = 8000 (80%)
        // So if licenseRequiredBond = 50000, need >= 40000 FOLD
    AND ticketToken.balanceOf(operator) / ticketPrice >= minTicketBalance
  )

  if (wasActive && !isNowActive):
    operators[operator].active = false
    numActiveOperators--
    emit OperatorActivationChanged(operator, false)

  if (!wasActive && isNowActive):
    operators[operator].active = true
    numActiveOperators++
    emit OperatorActivationChanged(operator, true)
```

Governance may update `ticketPrice`, `licenseRequiredBond`, `licenseActiveBps`, and
`minTicketBalance`; `minTicketBalance` must remain nonzero. Every effective update advances
`eligibilityConfigurationVersion`, resets `numActiveOperators`, and makes all previously cached
operator statuses fail closed. Operators or governance then call `refreshOperatorStatus` (or its
batch form) to re-evaluate registered operators under the new policy. Only operators refreshed into
the current version count as active, so committee requests cannot rely on status cached under an
older policy. The Rust sortition state consumes the same `ConfigurationUpdated` event and marks its
chain-local operators inactive until matching `OperatorActivationChanged` refresh events arrive.

A completed ban or unban refreshes the affected registered operator immediately.

---

## Step 2: Fund Tickets

The owner calls `addTicketBalanceFor(operator, amount)`: ticket collateral is pulled from the owner,
but non-transferable tFOLD is minted to the operator so committee snapshots remain keyed to the
node.

These steps are token operations, not an onboarding order. The operator must already be registered:
`addTicketBalanceFor` reverts with `NotRegistered()` otherwise. Onboarding runs bond, register, then
tickets — see [01_REGISTRATION.md](01_REGISTRATION.md#step-3-owner-funded-registration).

> **IMPORTANT:** The `amount` parameter is in **underlying token base units** (sUSDS share units at
> launch), NOT in ticket count. `ticketPrice` is only used in the activation check
> (`balanceOf / ticketPrice >= minTicketBalance`) and in sortition eligibility — it is NOT used to
> multiply the deposit amount.

```
Bond owner submits addTicketBalanceFor(operator, amount)
│
├─ 1. Approve collateral spend:
│     └─ ticketUnderlying.approve(ticketTokenAddress, amount)
│        → Note: approval is to the TicketToken contract (not BondingRegistry)
│        → because depositFrom pulls collateral into the TicketToken wrapper
│
├─ 2. BondingRegistry.addTicketBalanceFor(operator, amount)
│     │
│     │  ┌─── ON-CHAIN (BondingRegistry.sol) ──────────────────┐
│     │  │                                                      │
│     │  │  addTicketBalanceFor(operator, amount) {             │
│     │  │    1. require(msg.sender == bondOwnerOf(operator))   │
│     │  │    2. require(amount > 0)                            │
│     │  │    3. require(operators[operator].registered)        │
│     │  │    4. require(!exitInProgress(operator))             │
│     │  │    5. ticketToken.depositFrom(                       │
│     │  │         msg.sender,  // pull collateral from owner   │
│     │  │         operator,    // mint tFOLD to operator       │
│     │  │         amount       // raw underlying units         │
│     │  │       )              // NO ticketPrice multiplication│
│     │  │       │                                              │
│     │  │       │  ┌─ InterfoldTicketToken.depositFrom() ────┐  │
│     │  │       │  │  1. underlying.transferFrom(           │  │
│     │  │       │  │       from, address(this), amount)     │  │
│     │  │       │  │     → collateral moves: owner → tFOLD    │  │
│     │  │       │  │     → require tFOLD receives all amount │  │
│     │  │       │  │  2. _mint(to, amount)                  │  │
│     │  │       │  │     → minted amount = requested amount  │  │
│     │  │       │  │  3. Auto-delegate to self on first     │  │
│     │  │       │  │     deposit (for voting power tracking)│  │
│     │  │       │  └────────────────────────────────────────┘  │
│     │  │    6. _updateOperatorStatus(operator)                │
│     │  │    7. Emit TicketBalanceUpdated(operator,            │
│     │  │         +amount, newBalance, "DEPOSIT")              │
│     │  │  }                                                   │
│     │  └──────────────────────────────────────────────────────┘
│     │
└─ Operator receives tFOLD; owner retains lifecycle control
```

### Why tickets are non-transferable:

tFOLD tokens cannot be transferred between addresses. This ensures:

- An operator's collateral can't be moved to avoid slashing
- The ticket balance is always attributable to the specific operator
- Snapshot-based committee eligibility (checking balance at `requestBlock - 1`) is reliable

---

## Step 3: Unbond License

Only the configured owner may call `unbondLicenseFor(operator, amount)`. With a separate owner, the
operator's hot key cannot queue the owner's FOLD for exit.

```
Bond owner submits unbondLicenseFor(operator, 10000)
│
├─ BondingRegistry.unbondLicenseFor(operator, 10000)
│     │
│     │  ┌─── ON-CHAIN ─────────────────────────────────────────┐
│     │  │                                                       │
│     │  │  unbondLicenseFor(operator, amount) {                 │
│     │  │    1. require(msg.sender == bondOwnerOf(operator))    │
│     │  │    2. require(amount > 0)                             │
│     │  │    3. require(operators[operator].licenseBond         │
│     │  │              >= amount)                               │
│     │  │    4. operators[operator].licenseBond -= amount       │
│     │  │    5. _exits.queueLicensesForExit(                   │
│     │  │         operator, exitDelay, amount                   │
│     │  │       )                                               │
│     │  │       → Pending FOLD still counts in totalBonded()    │
│     │  │         until claimed or slashed                      │
│     │  │    6. _updateOperatorStatus(operator)                 │
│     │  │       → May DEACTIVATE if bond drops below threshold  │
│     │  │    7. Emit LicenseBondUpdated(operator, newBond)      │
│     │  │  }                                                    │
│     │  └───────────────────────────────────────────────────────┘
│
└─ Funds are now LOCKED for exitDelay seconds (time-locked exit)
```

---

## Step 4: Burn Tickets

Only the owner may call `removeTicketBalanceFor(operator, amount)`.

> **IMPORTANT:** Like `addTicketBalance`, the `amount` here is in **raw underlying token units**
> (tFOLD units, which are 1:1 with underlying). There is NO `ticketPrice` multiplication.

```
Bond owner submits removeTicketBalanceFor(operator, rawAmount)
│
├─ BondingRegistry.removeTicketBalanceFor(operator, rawAmount)
│     │
│     │  ┌─── ON-CHAIN ─────────────────────────────────────────┐
│     │  │                                                       │
│     │  │  removeTicketBalanceFor(operator, amount) {           │
│     │  │    1. require(msg.sender == bondOwnerOf(operator))    │
│     │  │    2. require(amount > 0)                             │
│     │  │    3. require(operators[operator].registered)         │
│     │  │    4. require(ticketToken.balanceOf(operator)         │
│     │  │              >= amount)                               │
│     │  │    5. ticketToken.burnTickets(operator, amount)       │
│     │  │       │  (NO ticketPrice multiplication — raw units)  │
│     │  │       │                                               │
│     │  │       │  ┌─ InterfoldTicketToken ───────────────────┐  │
│     │  │       │  │  burnTickets(operator, amount):        │  │
│     │  │       │  │    payableBalance += amount             │  │
│     │  │       │  │    _burn(operator, amount)             │  │
│     │  │       │  │    → tFOLD destroyed                     │  │
│     │  │       │  │    → Collateral is not returned yet    │  │
│     │  │       │  │    → Tracked in payableBalance for     │  │
│     │  │       │  │      later payout()                    │  │
│     │  │       │  └────────────────────────────────────────┘  │
│     │  │    6. _exits.queueTicketsForExit(                    │
│     │  │         operator, exitDelay, amount)                  │
│     │  │    7. _updateOperatorStatus(operator)                 │
│     │  │       → May DEACTIVATE if tickets drop below minimum  │
│     │  │    8. Emit TicketBalanceUpdated(operator,             │
│     │  │         -amount, newBalance, "WITHDRAW")              │
│     │  │  }                                                    │
│     │  └───────────────────────────────────────────────────────┘
│
└─ Tickets burned, collateral queued for exit after delay
```

---

## Step 5: Claim Exits

Both assets are always paid to `bondOwnerOf(operator)`, which may be the operator itself. Anyone can
settle matured ticket exits by passing `maxLicense = 0`. A claim that includes license collateral
must come from the bond owner. The exit queue remains keyed by operator, so queued assets stay
slashable against the correct protocol identity.

```text
Caller submits claimExitsFor(operator, maxTicket, maxLicense)
│
├─ BondingRegistry.claimExitsFor(operator, maxTicket, maxLicense)
│     │
│     │  ┌─── ON-CHAIN ─────────────────────────────────────────┐
│     │  │                                                       │
│     │  │  claimExitsFor(operator, maxTicket, maxLicense) {     │
│     │  │    1. if maxLicense != 0:                             │
│     │  │         require(msg.sender == bondOwnerOf(operator))  │
│     │  │    2. Reject active committee or slash obligations   │
│     │  │    3. (ticketAmount, licenseAmount) =                 │
│     │  │       _exits.claimAssets(                             │
│     │  │         operator, maxTicket, maxLicense               │
│     │  │       )                                               │
│     │  │       │                                               │
│     │  │       │  ┌─ ExitQueueLib.claimAssets() ───────────┐  │
│     │  │       │  │  Iterates tranches from head:          │  │
│     │  │       │  │  for each tranche where                │  │
│     │  │       │  │    block.timestamp >= unlockTimestamp:  │  │
│     │  │       │  │      take min(wanted, available)       │  │
│     │  │       │  │      from ticketAmount                  │  │
│     │  │       │  │  Skip locked tranches (future unlock)  │  │
│     │  │       │  │  Clean up empty tranches               │  │
│     │  │       │  │  Update pendingTotals                  │  │
│     │  │       │  └────────────────────────────────────────┘  │
│     │  │                                                       │
│     │  │    4. recipient = bondOwnerOf(operator)               │
│     │  │    5. if ticketAmount > 0:                            │
│     │  │       ticketToken.payout(recipient, ticketAmount)     │
│     │  │       │                                               │
│     │  │       │  ┌─ InterfoldTicketToken.payout() ──────────┐  │
│     │  │       │  │  Transfers collateral from             │  │
│     │  │       │  │  payableBalance to bond owner           │  │
│     │  │       │  │  payableBalance -= amount               │  │
│     │  │       │  │  underlying.safeTransfer(to, amount)    │  │
│     │  │       │  │  require owner receives exactly amount  │  │
│     │  │       │  └────────────────────────────────────────┘  │
│     │  │                                                       │
│     │  │    6. if licenseAmount > 0:                           │
│     │  │       totalLicenseLiability -= licenseAmount          │
│     │  │       licenseToken.safeTransfer(                       │
│     │  │         recipient, licenseAmount)                      │
│     │  │       → require owner receives exactly licenseAmount   │
│     │  │       → require registry spends exactly licenseAmount  │
│     │  │       → Pending FOLD is removed from totalBonded()    │
│     │  │         as returned FOLD reaches the wallet           │
│     │  │  }                                                    │
│     │  └───────────────────────────────────────────────────────┘
│
└─ Both assets are paid to the bond owner
```

Permissionless ticket settlement lets governance clear a matured ticket exit even when an operator
does not submit the payout transaction. A ticket-token registry change also requires both
`totalSupply()` and `payableBalance()` to be zero. A pending registry change freezes new deposits
and burns until governance activates or cancels it.

---

## Operator Voting Power

Bonding transfers FOLD to `BondingRegistry`, which never delegates it. Under ERC20Votes an
undelegated balance carries no voting power, so those votes are not moved to the registry — they
cease to exist. Bonded FOLD nonetheless still counts in `getPastTotalSupply`, so it raises the
quorum denominator while being unable to help meet it.

Two contracts restore that weight:

| Contract                     | Role                                                                               |
| ---------------------------- | ---------------------------------------------------------------------------------- |
| `registry/BondedCheckpoints` | Records `totalBonded(owner)` over time. Only `BondingRegistry` may write.          |
| `registry/BondedVotes`       | `IERC5805` view summing wallet voting power and bonded FOLD at the same timepoint. |

```
BondedVotes.getPastVotes(account, t)
│
├─ InterfoldToken.getPastVotes(account, t)        ← wallet voting power (needs delegation)
└─ BondedCheckpoints.getPastBonded(account, t)    ← FOLD bonded as an operator

BondedVotes.getPastTotalSupply(t)
└─ InterfoldToken.getPastTotalSupply(t)           ← passed through unchanged
```

Total supply is **not** adjusted. Bonded FOLD was transferred, not burned, so it is already in the
supply — adding it again would inflate every quorum denominator, which is the distortion this is
meant to remove.

### Checkpoint write sites

`BondingRegistry._syncBondedCheckpoint(bondOwner)` sends the owner's **current total**, not a delta,
so the history mirrors `_bondedByOwner`. A mutation site that forgets to call it is caught by
comparing `bonded(owner)` against `totalBonded(owner)` at the same instant, or a historical read
against the total expected at that timepoint; a delta-derived history would drift undetected, and it
would drift in voting weight. It is called from every site that mutates `_bondedByOwner`:

| Site                            | Trigger                                      |
| ------------------------------- | -------------------------------------------- |
| `_bondLicense`                  | bond                                         |
| `_decreaseDelegatedBond`        | slash                                        |
| `acceptBondOwner` (both owners) | bond-owner transfer                          |
| `_claimExits`                   | exit claim, mutated inside `BondingAssetLib` |

`unbondLicenseFor` is **not** a write site. Unbonding moves the bond into the exit queue, where the
FOLD is still held by the registry, so the delegated total is unchanged until the exit is claimed or
slashed. Voting power therefore stays with the owner for the duration of the exit window.

### Timepoints and configuration

`BondedCheckpoints` keys history by `block.timestamp`, matching `InterfoldToken`'s ERC-6372
`mode=timestamp` clock. `BondedVotes` compares the two clocks at construction and reverts on a
mismatch: summing a timestamp-keyed history with a block-numbered one would answer for two unrelated
points in time, and nothing downstream could detect it.

The constructor also reads `checkpoints.registry()` and requires that registry to bond the same
token it reads votes from. Matching clocks prove the history speaks the token's units, not that it
is _about_ that token — a history written by a registry custodying something else would add unbacked
weight to every vote. Because this calls the registry, `BondedVotes` cannot be built before the
registry is configured: `protocol/deployContracts` deploys `BondedCheckpoints` only, and
`--action activate-voting` deploys `BondedVotes` after the Safe batch executes.

`BondedVotes` also carries a read-only ERC-20 surface — `balanceOf`, `totalSupply`, `decimals`,
`name`, `symbol` — because Aragon's `TokenVotingSetup` gates installation on a `balanceOf` probe and
the governance app renders amounts from the metadata. There is no `transfer`, `transferFrom`,
`approve` or `allowance`: the contract owns no position, so a spend attempt reverts on a missing
selector. `balanceOf` subtracts `totalLicenseLiability` for the registry itself, because bonding
moves FOLD into the registry while the adapter attributes it to the bond owner, and counting it at
both addresses would put summed balances above total supply.

`setBondedCheckpoints` is settable **once per license token**, and verifies the candidate two ways
before the slot is spent. It must name this registry, and it must accept a write from it, which the
setter checks by synchronizing the zero address — whose bonded total is always zero, so a successful
probe leaves no readable state. Both checks are needed: `registry()` is not unique to a checkpoint
contract, and `InterfoldTicketToken` answers it with this same address, so an address mix-up would
otherwise consume the slot on a contract that cannot record anything and revert every later bond,
slash, exit claim and owner transfer with no way to correct it. While unset, `_syncBondedCheckpoint`
is a no-op rather than a revert: the registry is upgradeable, so the upgrade lands before the
contract can be pointed at it, and reverting in that window would freeze bonding, unbonding and
slashing.

Rotating the license token **detaches** the history, emitting `BondedCheckpointsDetached`. The
history counts license-token units, but `BondedVotes` adds them to the voting power of one token
fixed at construction; a replacement token's bonds would enter the same history, be counted as the
old token, and let summed voting power exceed that token's total supply. Rotation already requires
every old bond to be drained (`_validateLicenseAsset` rejects a non-zero registry balance), so each
owner's last recorded total is zero and detaching freezes a settled history rather than truncating a
live one. The detached contract keeps answering correctly for the timepoints it covers, and
governance may attach a fresh `BondedCheckpoints` — with a fresh `BondedVotes` bound to the new
token — for the next era. Repointing while one is attached is still refused.

Configuration does **not** backfill. An owner that bonded beforehand has no recorded history until
something mutates its total again, so until then it reads as holding nothing and votes only its
wallet balance. `resyncBondedCheckpoint(bondOwner)` records the owner's current total without
waiting for that mutation. It is permissionless and idempotent — it can only write the true current
total at the current timepoint, and cannot rewrite any past entry — so a third party may repair an
owner's history. Either call it for every existing owner after configuring, or configure the
checkpoint contract in the same transaction that upgrades the registry, before any bonding.

Bonded weight is **not delegatable** — the registry owns the position — so it always sits with the
bond owner, including for an owner that never self-delegated. Wallet-held FOLD keeps its normal
delegation through the token. `BondedVotes.delegate`/`delegateBySig` revert rather than silently
doing nothing.

## Activation Thresholds Summary

| Requirement           | Default             | Description                                |
| --------------------- | ------------------- | ------------------------------------------ |
| `licenseRequiredBond` | Configured by owner | Min FOLD to register                       |
| `licenseActiveBps`    | 8000 (80%)          | % of required bond to stay active          |
| `minTicketBalance`    | Configured by owner | Min tickets for active status              |
| `ticketPrice`         | Configured by owner | Stablecoin cost per ticket (in base units) |
| `exitDelay`           | Configured by owner | Seconds before exits can be claimed        |

### Activation formula:

```
active = registered
  AND licenseBond >= ceil(licenseRequiredBond * licenseActiveBps / 10000)
  AND (ticketToken.balanceOf(operator) / ticketPrice) >= minTicketBalance
```

---

## Token Flow Diagram

```
                BOND LICENSE                          BUY TICKETS
                ────────────                          ───────────
  Bond owner                               Bond owner
  FOLD wallet ──→ BondingRegistry          collateral wallet ──→ InterfoldTicketToken
                  (operator licenseBond++)                        (wraps asset → mints tFOLD)
                                                           tFOLD → Operator balance

               UNBOND LICENSE                         BURN TICKETS
               ──────────────                         ────────────
  licenseBond -= amount                    tFOLD burned from operator
  amount → ExitQueue (locked)              collateral stays in tFOLD contract (payableBalance)
                                           amount → ExitQueue (locked)

                              CLAIM EXITS
                              ───────────
                   After exitDelay seconds:
                   FOLD → returned to bond owner
                   collateral → bond owner from tFOLD.payableBalance
```

---

## Audit Cluster 2 Changes (Tokens)

The token contracts were hardened against the following audit findings. All changes are covered by
`packages/interfold-contracts/test/Token/` and have no runtime impact outside the touched contracts.

### InterfoldTicketToken (tFOLD)

- **Registry binding.** The initial circular deployment can use a placeholder registry only until
  the token is wired. Governance then repeats the atomic bonding-asset configuration check. Ticket
  asset activation requires `ticketToken.registry() == BondingRegistry`. A later mismatch makes an
  operator inactive. It does not block license slashes, bans, or exit bookkeeping.
- **H-02 — registry initialization.** The constructor now takes
  `(IERC20 baseToken, address registry_, address initialOwner_)` and assigns `registry = registry_`
  directly (emitting `RegistryChanged(0, registry_)`) instead of requiring the deployer to call
  `setRegistry()` later. Reverts `ZeroAddress` if `registry_ == 0`.
- **Exact-transfer deposits.** `depositFor` and `depositFrom` measure the underlying balance around
  `safeTransferFrom`. They revert unless the wrapper receives the full amount, then mint that same
  amount. Operators auto self-delegate on first deposit.
- **H-16 / H-20 / M-22 — registry swap timelock.** Once `lockRegistry()` is called (one-way,
  `RegistryLockAlreadySet` on repeat) further registry swaps must go through
  `requestRegistryChange(addr)` → wait `REGISTRY_CHANGE_DELAY = 1 day` → `activateRegistryChange()`.
  Errors: `RegistryNotLocked`, `RegistryChangeNotReady`, `NoPendingRegistry`,
  `RegistryAlreadyLocked`. `cancelRegistryChange()` clears the pending swap. The swap requires zero
  live ticket supply and zero payable balance. The pending period freezes new ticket liabilities.
- **M-11 — permit disabled.** `permit()` always reverts `PermitDisabled` so non-transferable tickets
  cannot be moved via off-chain signatures.
- **M-12 — rescueERC20.** `rescueERC20(token, to, amount)` lets the owner recover stray ERC-20s but
  refuses the underlying asset (`CannotRescueUnderlying`).
- **M-25 — delegation locked to self.** `delegate()` only accepts the caller's own address (else
  `DelegationLocked`); `delegateBySig` always reverts.
- **M-29 — EIP-6372 timestamp clock.** `clock() = uint48(block.timestamp)`,
  `CLOCK_MODE() = "mode=timestamp"`.

### InterfoldToken (FOLD) — Complete Rewrite

The FOLD token was rewritten to implement a CCA-auction-aligned lifecycle with wallet-level lock
enforcement based on immutable policy curves. Key changes:

- **Phase-based lifecycle.** The token derives its phase from immutable `CCA_START` / `CCA_END` and
  the one-way `tge()` call: Virtual → PublicSale → Cooldown → Live. Minting remains available in all
  pre-TGE phases. TGE is permissionless after `CCA_END + TGE_COOLDOWN` (40 days). The pre-TGE
  transfer gate automatically lifts at TGE. There is no manual transfer restriction flag.
- **Pre-TGE transfer gate.** Before TGE, only bonding-registry transfers, claim-source
  distributions, and whitelisted addresses can transfer. Bonding is always allowed so operators can
  stake during Virtual phase. The sale deployer removes the LiquidityLauncher from this whitelist
  after it distributes the sale and liquidity balances. The LBP strategy and position manager stay
  whitelisted because they move liquidity after the distribution transaction.
- **Immutable constructor parameters.** `CCA_START`, `CCA_END`, `CLAIM_SOURCE`, and
  `BONDING_REGISTRY` are set at construction and cannot change. The BondingRegistry must be deployed
  first (or a placeholder used and fixed via `setLicenseToken`).
- **License-token compatibility.** Each nonzero token passed to `setLicenseToken` must return one
  ABI-encoded `uint256` from `lockedBalanceOf(address)`. This keeps funded bond-owner transfers
  available after a token rotation.
- **Lock policy system.** `createLockPolicy(id, LockPolicy)` creates write-once policies with
  `Curve { anchor (Absolute|Tge), start, cliffDuration, vestDuration }` and optional `holdUntil`.
  `linkClaim(account, amount, policyId)` classifies pending claim-source tokens under a real policy.
  `PENDING_LOCK_POLICY_ID` holds unclassified claim tokens until linked.
- **Pooled wallet enforcement.** `lockedBalanceOf(account)` sums active locks (including PENDING).
  `transferableBalanceOf(account) = balance - max(0, locked - BONDING_REGISTRY.totalBonded(account))`.
  Transfers that exceed the transferable balance revert with `InsufficientUnlockedBalance`.
- **Claim-source auto-lock.** Tokens arriving from `CLAIM_SOURCE` are automatically locked as
  PENDING unless the recipient is in `lockWhitelist`. `linkClaim` moves PENDING to a real policy and
  queues unfilled amounts for future claims.
- **EIP-6372 timestamp clock.** `clock()` returns `block.timestamp`, `CLOCK_MODE()` is
  `"mode=timestamp"`.
- **Minting.** `mint(recipient, amount, label)` (DEFAULT_ADMIN_ROLE, unlocked) and
  `mintAllocations(MintAllocation[])` (MINTER_ROLE, locked to a policy) remain available during
  Virtual, PublicSale, and Cooldown. TGE permanently closes both functions.
- **Ownership.** `renounceOwnership()` is disabled. Two-step ownership transfer via Ownable2Step
  syncs all AccessControl roles atomically.

### Registry coordination

- `CiphernodeRegistryOwnable.requestBlock` stores `block.timestamp` (the storage slot and event
  field names remain unchanged for compatibility). Sortition reads tFOLD voting power at
  `requestBlock - 1`. The value is an EIP-6372 timepoint, not a block number, as required by the
  tFOLD timestamp clock.

### Node-operator event projection

The EVM reader now emits typed `BondOwnerSet`, `LicenseBondUpdated`, and
`CiphernodeDeregistrationRequested` events alongside the existing ticket and activation events. The
local dashboard rebuilds each chain's registered/active node sets and the local operator's bond
owner, ticket, license, and exit state from EventStore history; it does not parse human-oriented CLI
status output.
