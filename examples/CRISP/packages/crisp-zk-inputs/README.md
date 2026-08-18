# @crisp-e3/zk-inputs

WASM bindings for generating CRISP ZK proof inputs, compiled from Rust and shared between the
server-side Node.js environment and the browser. This package lets the CRISP SDK produce the circuit
witness data needed for Noir-based vote-validity proofs without duplicating the logic in TypeScript.

## What it generates

The WASM module wraps a `ZKInputsGenerator` class that performs BFV encryption and produces the
witness data needed for CRISP's Noir circuits.

`generateInputs` is the only entry point, and it covers all three operations: a first vote, a
re-vote, and a masker contribution under the
[vote masking](https://blog.theinterfold.com/vote-masking-receipt-freeness-secret-ballots/) scheme
that provides receipt-freeness.

```typescript
generateInputs(previousCiphertext, publicKey, vote, keepPrevious)
```

- `previousCiphertext` is the ciphertext currently in the slot, or `undefined` when it is empty.
- `keepPrevious` is set only for a mask over an occupied slot. It makes the ballot add to that
  ciphertext; everything else replaces it.

One function rather than one per operation, deliberately: the encryption, the witness, and the
published ciphertext have the same shape in every case, so nothing about a submission says which
operation it was. Splitting the paths would make the three tellable apart, which is the attack
masking exists to prevent.

The generator also exposes `encryptVote` / `decryptVote` for standalone BFV operations and
`generateKeys` for key generation.

These witness objects are then passed to `@noir-lang/noir_js` and `@aztec/bb.js` to generate the
actual ZK proofs.

## Usage

This package requires a universal init pattern because:

- In **Node.js** (>=18) WASM can be loaded synchronously — no preloading needed.
- In the **browser** the WASM binary must be fetched and instantiated asynchronously.

The `init` subpackage handles both environments transparently.

### ❌ Don't use the default export

```ts
// Bad — the raw default loader doesn't work in Node.js contexts
import init, { ZKInputsGenerator } from '@crisp-e3/zk-inputs'
```

### ✅ Use the universal subpackage loader

```ts
import init from '@crisp-e3/zk-inputs/init'
import { ZKInputsGenerator } from '@crisp-e3/zk-inputs'

await init()
const generator = ZKInputsGenerator.withDefaults()
const { encryptedVote, inputs } = generator.generateInputs(
  previousCiphertext,
  publicKey,
  vote,
  keepPrevious,
)
```

Call `init()` once before using any other imports from `@crisp-e3/zk-inputs`. In browser
environments `init()` fetches the WASM binary; in Node.js it is a no-op.

## Building

The WASM bundle is compiled from the Rust source in `crates/crisp-zk-inputs` using `wasm-pack`:

```bash
# From the CRISP root
pnpm build:wasm
```
