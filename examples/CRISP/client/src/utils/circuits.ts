// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { registeredPreset, setCircuits } from '@crisp-e3/sdk'

// The BFV-shaped circuits are ~2.9MB and ship as their own entry point per preset. Loading them
// through a dynamic import gives the bundler a split point, so the app boots on the ~350KB main
// entry and only pays for the circuits when someone actually votes.
//
// This client votes in insecure-512 rounds. Switching preset is a change here and nowhere else.
let pending: Promise<void> | null = null

/** Install the circuits needed for proving, at most once per session. */
export const ensureCircuits = async (): Promise<void> => {
  if (registeredPreset()) return

  pending ??= (async () => {
    try {
      const { loadCircuits } = await import('@crisp-e3/sdk/insecure-512')
      setCircuits(await loadCircuits())
    } catch (error) {
      // Let the next attempt retry rather than caching a failed fetch for the session.
      pending = null
      throw error
    }
  })()

  await pending
}
