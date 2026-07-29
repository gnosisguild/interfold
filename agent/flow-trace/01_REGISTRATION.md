# Part 1: Node Setup & Registration

## Overview

A ciphernode operator uses the CLI to configure local state, encrypt credentials, and authorize an
immutable bond owner. A separate wallet or Safe is recommended, but the operator may explicitly
choose itself. The configured owner then funds and registers the operator on-chain.

For non-interactive provisioning, `password set`, `wallet set`, and `ciphernode setup` expose
`--password-stdin` / `--private-key-stdin` alternatives. Container entrypoints use these stdin or
hidden-prompt paths so encryption passwords and private keys do not appear in process arguments or
environment metadata.

## Identity model: bond owner vs operator key

The on-chain operator remains the address whose key is loaded by the ciphernode. That address is
inserted into the registry, owns the non-transferable tFOLD voting balance, submits sortition
tickets, signs DKG proofs, and is the identity targeted by bans and slashes.

Before creating a position, the operator can run:

```text
interfold ciphernode set-bond-owner --owner 0xCOLD_WALLET
```

This sends `BondingRegistry.setBondOwner(owner)` from the operator key and emits the typed
`BondOwnerSet(operator, bondOwner)` event. The owner must be nonzero and may be the operator itself.
The authorization is immutable, and every collateral or registration action requires it.

Only that owner can call the financial/lifecycle `...For(operator)` entry points: `bondLicenseFor`,
`addTicketBalanceFor`, `registerOperatorFor`, `removeTicketBalanceFor`, `unbondLicenseFor`,
`deregisterOperatorFor`, and `claimExitsFor`. With the recommended separate-owner setup, the hot
operator key cannot fund, withdraw, deregister, or claim the position. The node CLI intentionally
exposes no bond, ticket, register, or exit transactions; the configured owner submits those calls
through the owner interface.

---

## Step 1: `interfold ciphernode setup`

**File:** `crates/cli/src/ciphernode/setup.rs` → delegates to
`crates/entrypoint/src/config/setup.rs`

### What happens call-by-call:

```
User runs: interfold ciphernode setup
│
├─ 1. Checks if config already exists → ABORTS if yes
│
├─ 2. Prompts for PASSWORD (confirmed twice)
│     └─ Stored encrypted via Cipher → written to local keystore
│        File: ~/.config/interfold/<name>/password (encrypted blob)
│
├─ 3. Prompts for WEBSOCKET RPC URL
│     └─ Default: wss://ethereum-sepolia-rpc.publicnode.com
│     └─ Validates it's a valid URL
│
├─ 4. Prompts for ETHEREUM PRIVATE KEY (hex)
│     └─ Encrypted with Cipher using the password from step 2
│     └─ Stored in local keystore
│     └─ NEVER stored in plaintext
│
├─ 5. Prompts for CONFIG DIRECTORY
│     └─ Default: ~/.config/interfold
│
├─ 6. Creates config file (YAML):
│     chains:
│       - name: "default"
│         rpc_url: <user's URL>
│         contracts:
│           interfold: <address>
│           bonding_registry: <address>
│           ciphernode_registry: <address>
│           slashing_manager: <address>
│
├─ 7. Derives and prints:
│     └─ Node ADDRESS (from private key)
│     └─ Peer ID (libp2p identity derived from private key)
│
└─ OUTPUT: "Setup complete. Your address: 0x... Your peer ID: 12D3Koo..."
```

### Key internals:

- **Cipher** (`crates/crypto/src/`): AES-256-GCM encryption. The password is used to derive an
  encryption key via Argon2. All secrets at rest are encrypted.
- **Config** (`crates/config/src/`): YAML-based `AppConfig` struct with chain configurations,
  contract addresses, node role, peers, etc.

---

## Step 2: Authorize the bond owner

**File:** `crates/cli/src/ciphernode/lifecycle.rs` → `set_bond_owner()`

```
Operator runs:
  interfold ciphernode set-bond-owner --owner 0xCOLD_WALLET
│
└─ BondingRegistry.setBondOwner(owner)
   ├─ Rejects the zero address
   ├─ Allows owner == operator (separate owner recommended)
   ├─ Rejects a second assignment
   ├─ Stores bondOwners[operator] = owner
   └─ Emits BondOwnerSet(operator, owner)
```

Until this transaction is mined, `bondOwnerOf(operator)` returns the zero address and all
owner-authorized position calls fail.

---

## Step 3: Owner-funded registration

The bond owner wallet or Safe performs the on-chain position transactions. These transactions are
not node CLI commands because the CLI contains only the hot operator key.

```
Bond owner
│
├─ licenseToken.approve(BondingRegistry, bondAmount)
├─ BondingRegistry.bondLicenseFor(operator, bondAmount)
├─ stablecoin.approve(InterfoldTicketToken, ticketAmount)
├─ BondingRegistry.addTicketBalanceFor(operator, ticketAmount)
└─ BondingRegistry.registerOperatorFor(operator)
   ├─ Verifies msg.sender == bondOwnerOf(operator)
   ├─ Verifies the operator is not banned or already registered
   ├─ Verifies the operator has the required FOLD bond
   ├─ Sets operators[operator].registered = true
   ├─ Calls registry.addCiphernode(operator)
   │  ├─ Inserts uint160(operator) into the Lean IMT
   │  └─ Emits CiphernodeAdded(operator)
   └─ Calls _updateOperatorStatus(operator)
      └─ Activates when bond and ticket thresholds are met
```

The node's address—not the bond owner's—is inserted into the IMT, owns the tFOLD balance, and
remains the committee and slashing identity.

---

## Step 4: `interfold ciphernode status`

**File:** `crates/cli/src/ciphernode/lifecycle.rs` → `status()`

```
User runs: interfold ciphernode status
│
├─ ChainContext::new()
│
├─ Reads on-chain state (multiple view calls):
│   ├─ operator.registered
│   ├─ operator.active
│   ├─ operator.exitRequested
│   ├─ ticketToken.balanceOf(address) → ticket balance
│   ├─ operator.licenseBond → license bond amount
│   ├─ bondingRegistry.bondOwnerOf(address) → collateral owner
│   ├─ pendingExits.ticketAmount, pendingExits.licenseAmount
│   ├─ bondingRegistry.minTicketBalance → required minimum
│   ├─ bondingRegistry.ticketPrice → price per ticket
│   └─ bondingRegistry.licenseRequiredBond → required bond
│
└─ OUTPUT:
   Operator Key:     0x1234...
   Bond Owner:       0xabcd...
   Registered:       true
   Active:           true
   Exit Pending:     false
   Ticket Balance:   100 (available: 95)
   License Bond:     50000 FOLD
   Pending Exits:    tickets=0, license=0
   Requirements:     minTickets=10, ticketPrice=1000000, licenseBond=50000
```

---

## Rust-Side: What Happens When a Running Node Detects Registration

When a ciphernode is running (`interfold start`), its EVM readers are listening for on-chain events:

```
BondingRegistrySolReader detects OperatorActivationChanged event
│
├─ Publishes to EventBus: OperatorActivationChanged { node, active }
│
├─ Sortition actor receives event:
│   ├─ If active=true: adds node to NodeStateStore as eligible
│   └─ If active=false: removes node from eligible set
│
└─ This node is now part of the sortition pool for future E3 committees
```

`BondOwnerSet` is also decoded into a typed Rust event. The dashboard records the owner for the
local operator, while `ciphernode status` reads it directly from `bondOwnerOf`.

```
CiphernodeRegistrySolReader detects CiphernodeAdded event
│
├─ Publishes to EventBus: CiphernodeAdded { node }
│
└─ Sortition actor: updates IMT root tracking
```

---

## Contract Interaction Diagram

```
┌────────────────┐ registerOperatorFor(operator) ┌──────────────────┐
│ Bond owner/Safe│ ─────────────────────────────→ │  BondingRegistry │
└────────────────┘                                └────────┬─────────┘
                                                      │
                                          addCiphernode(node)
                                                      │
                                                      ▼
                                             ┌────────────────────────┐
                                             │ CiphernodeRegistry     │
                                             │ (Lean IMT insert)      │
                                             │                        │
                                             │ Emits:                 │
                                             │  CiphernodeAdded       │
                                             └────────────────────────┘
                                                      │
                                          _updateOperatorStatus()
                                                      │
                                                      ▼
                                             ┌────────────────────────┐
                                             │  If meets thresholds:  │
                                             │  active = true         │
                                             │  numActiveOperators++  │
                                             │                        │
                                             │  Emits:                │
                                             │  OperatorActivation    │
                                             │  Changed(node, true)   │
                                             └────────────────────────┘
```
