#!/usr/bin/env node
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Refuse to publish a channel whose artifacts do not match the preset that channel stands for.
//
// The release channels are split by BFV preset: `testing` carries insecure-512 and `latest` carries
// secure-8192. A tarball therefore has to carry exactly one preset — the missing one must be
// missing, so importing it fails to resolve, and the wrong one must be absent, so a prod consumer
// cannot reach insecure parameters at all.
//
// The SDK and the contracts package are checked together because they are a matched pair. The SDK
// inlines the compiled circuit and the contracts package ships the verifier generated from that
// same circuit's verification key, so a channel that mixes them fails on chain at proof
// verification rather than anywhere a test would catch it.

import { existsSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const CRISP = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SDK = join(CRISP, 'packages', 'crisp-sdk')
const CONTRACTS = join(CRISP, 'packages', 'crisp-contracts')

/** Which preset each release channel stands for. */
const CHANNEL_PRESET = { testing: 'insecure-512', latest: 'secure-8192' }

/** Generated per census mode. Not preset-specific — see packages/crisp-contracts/scripts/verifiers.ts. */
const VERIFIERS = ['CRISPVerifier.sol', 'CRISPOnchainVerifier.sol']

/** Polynomial degree each preset's circuits carry, used to prove the bundle is what it claims. */
const EXPECTED_DEGREE = { 'insecure-512': 512, 'secure-8192': 8192 }

/** Below this a "built" entry is a stub or a failed inline rather than a real circuit bundle. */
const MIN_BYTES = 100 * 1024

// npm gives `prepublishOnly` no way to see `--tag`, so the channel comes through the environment
// when the publish scripts set it, and through argv when run by hand.
const channel = process.argv[2] ?? process.env.CRISP_CHANNEL
if (!Object.hasOwn(CHANNEL_PRESET, channel)) {
  console.error(
    channel === undefined
      ? `Set CRISP_CHANNEL or pass a channel: check-presets.mjs <${Object.keys(CHANNEL_PRESET).join('|')}>`
      : `Unknown channel "${channel}"; expected one of ${Object.keys(CHANNEL_PRESET).join(', ')}.`,
  )
  process.exit(1)
}

const wanted = CHANNEL_PRESET[channel]
const others = Object.values(CHANNEL_PRESET).filter((preset) => preset !== wanted)
const problems = []

// --- the SDK bundle ---
const built = join(SDK, 'dist', 'presets', `${wanted}.js`)
if (!existsSync(built)) {
  problems.push(`${wanted}: dist/presets/${wanted}.js is missing — run \`pnpm build:presets\`, then \`CRISP_PRESET=${wanted} pnpm build\`.`)
} else if (statSync(built).size < MIN_BYTES) {
  problems.push(`${wanted}: dist/presets/${wanted}.js is only ${statSync(built).size} bytes; the circuits did not inline.`)
}

for (const other of others) {
  if (existsSync(join(SDK, 'dist', 'presets', `${other}.js`))) {
    problems.push(`${other}: dist/presets/${other}.js must not ship on "${channel}" — rebuild with CRISP_PRESET=${wanted}.`)
  }
}

// The exports map is what consumers actually resolve, so check it points at a real file.
const pkg = JSON.parse(readFileSync(join(SDK, 'package.json'), 'utf8'))
const entry = pkg.exports?.[`./${wanted}`]
if (!entry) {
  problems.push(`${wanted}: package.json exports has no "./${wanted}" subpath.`)
} else {
  for (const target of new Set(Object.values(entry))) {
    if (!existsSync(join(SDK, target))) problems.push(`${wanted}: exports points at ${target}, which does not exist.`)
  }
}

// --- the bundle really carries the channel's circuits ---
//
// The filename says which preset a bundle is; this checks the contents agree. A bundle built from
// the wrong staged artifacts would ship under the right name and fail only at on-chain
// verification, which is the whole failure mode these channels exist to prevent.
if (existsSync(built)) {
  const source = readFileSync(built, 'utf8')
  const degree = EXPECTED_DEGREE[wanted]
  const other = Object.entries(EXPECTED_DEGREE).find(([preset]) => preset !== wanted)

  // The bundler may emit the inlined JSON as a JS object literal, so the key can be quoted or bare
  // and the space is optional. Match all of those rather than one spelling.
  const hasDegree = (value) => new RegExp(`["']?length["']?\\s*:\\s*${value}\\b`).test(source)

  if (!hasDegree(degree)) {
    problems.push(`${wanted}: dist/presets/${wanted}.js contains no length-${degree} arrays; the inlined circuits are not ${wanted}.`)
  }
  if (other && hasDegree(other[1])) {
    problems.push(`${wanted}: dist/presets/${wanted}.js contains length-${other[1]} arrays, which belong to ${other[0]}.`)
  }
}

// --- the generated verifiers ---
//
// Preset-independent, so this is only an existence check: without them a consumer has nothing to
// deploy, but there is no wrong-preset case to catch here.
for (const verifier of VERIFIERS) {
  if (!existsSync(join(CONTRACTS, 'contracts', 'verifiers', verifier))) {
    problems.push(`contracts/verifiers/${verifier} is missing — regenerate with scripts/compile_circuits.sh.`)
  }
}

if (problems.length > 0) {
  console.error(`✗ Not publishable on "${channel}" (expects ${wanted}):`)
  for (const problem of problems) console.error(`  - ${problem}`)
  process.exit(1)
}

const size = (statSync(built).size / 1048576).toFixed(1)
console.log(`✓ "${channel}" carries ${wanted} only (dist/presets/${wanted}.js ${size}MB, verifiers present).`)
