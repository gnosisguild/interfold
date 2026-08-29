// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { existsSync } from 'node:fs'
import { defineConfig } from 'tsup'

// Each preset is its own entry point, so a consumer's bundler can load only the BFV-shaped circuits
// the current round needs.
//
// `CRISP_PRESET` builds one preset. The testing channel uses that to keep the package lightweight.
// With `CRISP_PRESET` unset, every staged preset becomes an entry point. Production releases use
// that path so one client can serve both secure mainnet rounds and insecure testnet rounds.
//
// With `CRISP_PRESET` unset the build takes whatever is staged, which is what local development
// wants.
const PRESETS = ['insecure-512', 'secure-8192']

const staged = (preset) => existsSync(`../../circuits/dist/${preset}/crisp.json`)

const requested = process.env.CRISP_PRESET
if (requested !== undefined && !PRESETS.includes(requested)) {
  throw new Error(`CRISP_PRESET must be one of ${PRESETS.join(', ')}; got "${requested}".`)
}

let selected
if (requested === undefined) {
  selected = PRESETS.filter(staged)
  for (const preset of PRESETS) {
    if (!selected.includes(preset)) console.warn(`⚠️  tsup: skipping "${preset}" entry — circuits/dist/${preset} is not staged.`)
  }
  if (selected.length === 0) throw new Error('No circuit preset staged. Run `pnpm build:presets` before building the SDK.')
} else {
  if (!staged(requested)) {
    throw new Error(`CRISP_PRESET=${requested} but circuits/dist/${requested} is not staged. Run \`pnpm build:presets\` first.`)
  }
  selected = [requested]
  console.log(`tsup: building the ${requested} entry only (CRISP_PRESET).`)
}

export default defineConfig({
  entry: ['src/index.ts', 'src/workers/generateCircuitInputs.worker.ts', ...selected.map((preset) => `src/presets/${preset}.ts`)],
  include: ['src/**/*.ts'],
  splitting: false,
  sourcemap: true,
  clean: true,
  format: ['esm'],
  dts: true,
})
