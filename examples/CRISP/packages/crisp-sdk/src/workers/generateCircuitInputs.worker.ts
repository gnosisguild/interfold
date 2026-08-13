// SPDX-License-Identifier: LGPL-3.0-only
//
// Runs prepareCircuitInputs in a worker to avoid blocking the main thread
// during CPU-heavy zk-inputs WASM (BFV encryption).

import type { PrepareBallotInputs } from '../types'
import { prepareCircuitInputsImpl } from '../circuitInputs'

self.onmessage = async (e: MessageEvent<PrepareBallotInputs>) => {
  try {
    const prepared = await prepareCircuitInputsImpl(e.data)
    self.postMessage({ type: 'result' as const, prepared })
  } catch (err) {
    const error = err instanceof Error ? err.message : String(err)
    const stack = err instanceof Error ? err.stack : undefined
    self.postMessage({ type: 'error' as const, error, stack })
  }
}
