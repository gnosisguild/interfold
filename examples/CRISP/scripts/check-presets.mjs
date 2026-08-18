#!/usr/bin/env node
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Refuse to publish an SDK or contracts package that carries only some of its presets.
//
// tsup skips a preset whose artifacts are not staged, and it warns rather than fails so an ordinary
// development build still works from one preset. That is the right default locally and the wrong
// one at publish time: the missing subpath is only discovered by the consumer who imports it.
//
// The two packages are checked together because they are a matched pair. The SDK inlines the
// compiled circuit and the contracts package ships the verifier generated from that same circuit's
// verification key, so publishing one preset's SDK against another's verifier fails on chain at
// proof verification rather than anywhere a test would catch it.

import { existsSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const CRISP = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SDK = join(CRISP, 'packages', 'crisp-sdk')
const CONTRACTS = join(CRISP, 'packages', 'crisp-contracts')
const PRESETS = ['insecure-512', 'secure-8192']

/** Generated per census mode; both are needed for a preset to be deployable. */
const VERIFIERS = ['CRISPVerifier.sol', 'CRISPOnchainVerifier.sol']

/** Below this a "built" entry is a stub or a failed inline rather than a real circuit bundle. */
const MIN_BYTES = 100 * 1024

const problems = []
for (const preset of PRESETS) {
  const built = join(SDK, 'dist', 'presets', `${preset}.js`)
  if (!existsSync(built)) {
    problems.push(`${preset}: dist/presets/${preset}.js is missing — run \`pnpm build:presets\` then rebuild.`)
    continue
  }

  const { size } = statSync(built)
  if (size < MIN_BYTES) {
    problems.push(`${preset}: dist/presets/${preset}.js is only ${size} bytes; the circuits did not inline.`)
  }
}

// The exports map is what consumers actually resolve, so check it points at real files.
const pkg = JSON.parse(await import('node:fs').then((fs) => fs.readFileSync(join(SDK, 'package.json'), 'utf8')))
for (const preset of PRESETS) {
  const entry = pkg.exports?.[`./${preset}`]
  if (!entry) {
    problems.push(`${preset}: package.json exports has no "./${preset}" subpath.`)
    continue
  }

  for (const target of new Set(Object.values(entry))) {
    if (!existsSync(join(SDK, target))) problems.push(`${preset}: exports points at ${target}, which does not exist.`)
  }
}

// The generated verifiers are what a consumer deploys, and they are preset-specific because they
// encode the circuit's verification key.
for (const preset of PRESETS) {
  for (const verifier of VERIFIERS) {
    const path = join(CONTRACTS, 'contracts', 'verifiers', preset, verifier)
    if (!existsSync(path)) {
      problems.push(`${preset}: contracts/verifiers/${preset}/${verifier} is missing — regenerate with scripts/compile_circuits.sh.`)
    }
  }
}

if (problems.length > 0) {
  console.error('✗ Not publishable with both presets:')
  for (const problem of problems) console.error(`  - ${problem}`)
  process.exit(1)
}

const sizes = PRESETS.map((p) => `${p} ${(statSync(join(SDK, 'dist/presets', `${p}.js`)).size / 1048576).toFixed(1)}MB`)
console.log(`✓ both presets present: ${sizes.join(', ')}`)
