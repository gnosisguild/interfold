// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { existsSync } from 'node:fs'
import { defineConfig } from 'tsup'

// Each preset is its own entry point so a consumer's bundler pulls one set of BFV-shaped circuits
// rather than both. A preset is built only once its artifacts have been staged, because the working
// tree holds a single preset at a time — `pnpm build:presets` compiles and stages each in turn.
const PRESETS = ['insecure-512', 'secure-8192']
const staged = PRESETS.filter((preset) => existsSync(`../../circuits/dist/${preset}/crisp.json`))

for (const preset of PRESETS) {
  if (!staged.includes(preset)) {
    console.warn(`⚠️  tsup: skipping "${preset}" entry — circuits/dist/${preset} is not staged.`)
  }
}

if (staged.length === 0) {
  throw new Error('No circuit preset staged. Run `pnpm build:presets` before building the SDK.')
}

export default defineConfig({
  entry: [
    'src/index.ts',
    'src/workers/generateCircuitInputs.worker.ts',
    ...staged.map((preset) => `src/presets/${preset}.ts`),
  ],
  include: ['src/**/*.ts'],
  splitting: false,
  sourcemap: true,
  clean: true,
  format: ['esm'],
  dts: true,
})
