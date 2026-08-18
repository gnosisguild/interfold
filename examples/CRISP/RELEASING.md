# Releasing the CRISP packages

`@crisp-e3/sdk` and `@crisp-e3/contracts` are published as a matched pair on two channels, split by
BFV preset.

| channel    | tag       | preset         | who it is for                      |
| ---------- | --------- | -------------- | ---------------------------------- |
| testing    | `testing` | `insecure-512` | testnets, demos, local development |
| production | `latest`  | `secure-8192`  | real rounds                        |

## Why the channels are split by preset

The SDK inlines the compiled circuit and the contracts package ships the Solidity verifier generated
from that same circuit's verification key. A verifier only accepts proofs from the circuit it was
generated for, so mixing presets across the two packages produces a round that rejects every ballot
— and it fails at on-chain verification, not anywhere a test would catch it.

Each tarball therefore carries exactly one preset. Importing the other subpath fails to resolve,
which is a loud, immediate error rather than a silently wrong proof:

```ts
// on @crisp-e3/sdk@testing
import { loadCircuits } from '@crisp-e3/sdk/insecure-512' // resolves
import { loadCircuits } from '@crisp-e3/sdk/secure-8192' // ERR_MODULE_NOT_FOUND
```

It also keeps the secure circuits out of every testing install. They are far larger than the
insecure ones, and shipping both would put that weight in both channels.

## Versioning

Testing releases carry a prerelease identifier; production releases do not.

```
0.18.0-insecure.0   tag: testing    insecure-512
0.18.0              tag: latest     secure-8192
```

The identifier is load-bearing. npm excludes prerelease versions from ordinary ranges, so a consumer
on `^0.18.0` can never drift onto a testing build through an update — reaching it takes an explicit
`@testing` or an exact version.

## Procedure

Both channels build from artifacts staged by `pnpm build:presets`, which compiles each preset in
turn and writes the generated verifiers to `packages/crisp-contracts/contracts/verifiers/<preset>/`.
Run it once, then publish each channel:

```sh
pnpm -C examples/CRISP build:presets          # slow: compiles both presets

# testing
cd examples/CRISP/packages/crisp-sdk
npm version 0.18.0-insecure.0 --no-git-tag-version
pnpm publish:testing
cd ../crisp-contracts
npm version 0.18.0-insecure.0 --no-git-tag-version
pnpm publish:testing

# production
cd ../crisp-sdk
npm version 0.18.0 --no-git-tag-version
pnpm publish:prod
cd ../crisp-contracts
npm version 0.18.0 --no-git-tag-version
pnpm publish:prod
```

`prepublishOnly` runs `check-presets` on both packages, which refuses to publish a channel whose
artifacts do not match its preset — a missing preset, a stub bundle, an exports entry pointing at
nothing, missing verifiers, or the _other_ preset's bundle being present.

## Deploying

The contracts tarball carries the verifiers for both presets, because they are small and pruning
them buys nothing. Safety comes from the deploy path refusing to guess instead: `activePreset()` in
`packages/crisp-contracts/scripts/verifiers.ts` uses `CRISP_PRESET` when set, falls back only when
exactly one preset has generated verifiers, and otherwise throws. There is no default — quietly
deploying the insecure verifier is the mistake worth making impossible.

```sh
CRISP_PRESET=secure-8192 pnpm deploy:crisp --network <network>
```
