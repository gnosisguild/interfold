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

Inside the monorepo the pin does not decide anything — `linkWorkspacePackages: true` links
`packages/crisp-sdk` whatever the specifier says. It decides the standalone deploy, which installs
with `ignore-workspace=true` (see `client/.npmrc`) and resolves from npm through its own lock file.

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
all three packages, builds the SDK against the channel's preset, publishes in dependency order
(`zk-inputs`, `sdk`, `contracts`), and commits — it never tags and never pushes.

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
