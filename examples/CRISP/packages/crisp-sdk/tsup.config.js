// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { existsSync } from 'node:fs'
import { defineConfig } from 'tsup'

// Each preset is its own entry point, so a consumer's bundler pulls one set of BFV-shaped circuits
// rather than both.
//
// A published tarball carries exactly one preset, selected by `CRISP_PRESET`, because the release
// channels are split by preset: `testing` ships insecure-512 and `latest` ships secure-8192. The
// secure circuits are far larger than the insecure ones, and shipping both would put that weight in
// every install of either. Importing the preset a tarball does not carry then fails to resolve,
// which is the right failure — the alternative is proving against parameters the deployed verifier
// does not match, and that is only discovered on chain.
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
