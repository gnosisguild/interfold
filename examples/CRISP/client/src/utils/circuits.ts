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
// This testnet client carries only the insecure preset. Production clients install the `latest`
// SDK release and import its secure preset instead. Check the round before proving so a deployment
// mismatch fails here instead of producing a proof that the on-chain verifier rejects.
let pending: Promise<void> | null = null

/** Install the circuits needed for proving, at most once per session. */
export const ensureCircuits = async (paramSet: number): Promise<void> => {
  if (paramSet !== 0) {
    throw new Error(
      `This CRISP client carries insecure-512 circuits, but E3 param set ${paramSet} requires secure-8192. Use the production client built with @crisp-e3/sdk@latest.`,
    )
  }

  const activePreset = registeredPreset()
  if (activePreset) {
    if (activePreset !== 'insecure-512') {
      throw new Error(`The registered ${activePreset} circuits do not match E3 param set ${paramSet}.`)
    }
    return
  }

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

  if (registeredPreset() !== 'insecure-512') {
    throw new Error('The loaded circuit bundle does not match E3 param set 0.')
  }
}
