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

## The client stays on testing

`client/` is deployed against a testnet whose verifiers were deployed from `insecure-512` circuits,
so it always pins a testing version. Only a testing publish moves it; a production publish leaves
`client/package.json` and `client/pnpm-lock.yaml` untouched, and refuses to run at all if something
else has moved the pin onto a plain release version.

The pin cannot be a `workspace:` range, because the same `client/package.json` also serves the
standalone Vercel deploy, which installs with `ignore-workspace=true` (see `client/.npmrc`) and
cannot resolve one. So it is an exact version, and that has a consequence worth stating plainly:

**`linkWorkspacePackages: true` links `packages/crisp-sdk` only while its version still satisfies
that exact pin.** Bump the workspace to `0.18.0` while the client pins `0.18.0-insecure.0` and pnpm
stops linking and resolves the client from the registry instead. That is not a cosmetic difference.
It re-resolves the whole client dependency tree (it pulled `zod@4` in place of `zod@3` across
`viem`, `wagmi`, and `connectkit`), and it makes Vite pre-bundle the SDK as an ordinary dependency,
which breaks the worker subpath:

```
The file does not exist at ".../client/node_modules/.vite/deps/
workers/generateCircuitInputs.worker.js?worker_file&type=module"
→ [generateProof] failed → the e2e vote never reaches the wallet signature
```

This is why a production publish restores the versions it bumped instead of committing them, and why
`bumpsRepo` in `publish.ts` is tied to `tracksClient` rather than being independently selectable. A
published version lives on npm; it does not need to live in the working tree.

Moving the client to a production version is a deliberate change, made together with the redeploy
that gives it `secure-8192` verifiers to prove against.

## Procedure

Both channels build from the artifacts that `pnpm build:presets` archives under
`circuits/dist/<preset>/`, which git does not track. Run it whenever the circuits changed; the SDK
build refuses artifacts that are missing or older than the sources, comparing a content digest that
`stage-preset-artifacts.mjs` records at staging time.

```sh
pnpm -C examples/CRISP build:presets          # slow: compiles both presets
cd examples/CRISP

# testing — insecure-512 under the `testing` tag, and moves the client
pnpm publish:packages --channel testing 0.19.0-insecure.0

# production — secure-8192 under the `latest` tag, and leaves the client alone
pnpm publish:packages --channel prod 0.19.0
```

Add `--dry-run` to print the exact steps for a channel without changing anything. The script bumps
all three packages, builds the SDK against the channel's preset, and publishes in dependency order
(`zk-inputs`, `sdk`, `contracts`). It never tags and never pushes.

What it leaves behind differs by channel. Testing commits the bump. Production restores it, so the
tree ends exactly as it started — see "The client stays on testing" above for why. Either way, a
failure at any point restores the tree too: a half-bumped workspace is worse than no bump, because
the next `pnpm install` silently detaches the client from it.

Two gates run on the way:

- `check-staged-preset.mjs` (in `build:testing` / `build:prod`) refuses staged artifacts the
  circuits have moved past.
- `check-presets.mjs` (`prepublishOnly` on both packages) refuses to publish a channel whose
  artifacts do not match its preset — a missing preset, a stub bundle, an exports entry pointing at
  nothing, missing verifiers, or the _other_ preset's bundle being present.

## Deploying

The generated Solidity verifiers are **not** preset-specific, so there is one set of them and no
choice to get wrong at deploy time. `compile_circuits.sh` generates each from its fold circuit's
verification key; the fold circuit takes the inner key as an input and checks its hash against
either preset's constant, so its own structure carries no BFV degree and a single verifier accepts
proofs from either preset. Compiling both presets produces byte-identical verifier sources.

What must match is the SDK a deployment is used with: a round proves with whichever preset the
installed SDK carries, and the ciphernodes decrypt with the preset their deployment was configured
for. Pair a `latest` SDK with a `secure-8192` deployment and a `testing` SDK with an `insecure-512`
one.
