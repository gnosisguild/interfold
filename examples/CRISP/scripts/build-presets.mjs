#!/usr/bin/env node
// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Compile and stage the CRISP circuits for every preset, so the SDK can publish both.
//
// The preset is a global compile-time choice (circuits/lib/src/configs/default/mod.nr), so the
// presets cannot be built concurrently — each pass switches the tree, compiles, and archives the
// result under circuits/dist/<preset>/ before the next pass overwrites it.
//
// The threshold circuits go through the root builder rather than a bare `nargo compile`, because
// switching preset also regenerates the parity matrices and ActiveCryptoConfig.sol. The root
// builder does not know about the CRISP circuits, so those are compiled here afterwards.

import { execFileSync } from 'node:child_process'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const CRISP = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const REPO = resolve(CRISP, '..', '..')

/**
 * Insecure is built last so the working tree is left on the default preset.
 *
 * A plain `pnpm build` in the SDK inlines whatever the tree currently holds, so ending on anything
 * else would quietly change what the next build produces.
 */
const PRESETS = ['secure-8192', 'insecure-512']

const run = (cmd, args, cwd) => {
  console.log(`   $ ${cmd} ${args.join(' ')}`)
  execFileSync(cmd, args, { cwd, stdio: 'inherit' })
}

for (const preset of PRESETS) {
  console.log(`\n🔮 Building CRISP circuits for ${preset}...`)

  // Switches the tree to this preset. This is the step that also regenerates the parity matrices
  // and ActiveCryptoConfig.sol, which a bare `nargo compile` would leave describing the old one.
  run('pnpm', ['tsx', 'scripts/build-circuits.ts', '--preset', preset, '--group', 'threshold'], REPO)

  // Compiles the CRISP circuits and writes the generated Solidity verifiers into
  // packages/crisp-contracts/contracts/verifiers/<preset>/. It recompiles the threshold circuits on
  // the way through, which duplicates part of the step above; that is worth the few minutes rather
  // than splitting the verifier generation away from the compile it has to agree with.
  run('bash', ['scripts/compile_circuits.sh'], CRISP)

  run('node', [join(CRISP, 'scripts/stage-preset-artifacts.mjs'), preset], CRISP)
}

console.log(`\n✓ staged ${PRESETS.length} preset(s); tree left on ${PRESETS.at(-1)}`)
console.log('  If the ballot circuits changed, regenerate the fold key hashes: scripts/compute_vk_hash.sh')
console.log('  Then rebuild the SDK (pnpm -C packages/crisp-sdk build) and check: pnpm -C packages/crisp-sdk check:presets')
