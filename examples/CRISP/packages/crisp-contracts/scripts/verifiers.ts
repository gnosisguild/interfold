// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Fully qualified names for the generated Honk verifiers.
//
// The verifiers are generated from the compiled circuits, so they exist once per BFV preset and
// live under contracts/verifiers/<preset>/. With two presets there are four files declaring
// `HonkVerifier`, `ZKTranscriptLib` and `RelationsLib`, so every lookup has to be qualified by
// file — a bare name is ambiguous and hardhat rejects it.
//
// Both the deploy script and the tests resolve names through here, so the preset is chosen in one
// place rather than spelled into a dozen string literals.

import { existsSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** BFV parameter sets the circuits can be compiled against. */
export type CircuitPreset = 'insecure-512' | 'secure-8192'

export const CIRCUIT_PRESETS: readonly CircuitPreset[] = ['insecure-512', 'secure-8192']

/**
 * Which census a round establishes eligibility with.
 *
 * `merkle` proves membership of a census tree; `onchain` reads voting power from a token. They are
 * separate circuits and therefore separate verifiers, each with its own libraries.
 */
export type VerifierVariant = 'merkle' | 'onchain'

const VERIFIER_FILES: Record<VerifierVariant, string> = {
  merkle: 'CRISPVerifier.sol',
  onchain: 'CRISPOnchainVerifier.sol',
}

const verifierDir = (preset: CircuitPreset) =>
  resolve(dirname(fileURLToPath(import.meta.url)), '..', 'contracts', 'verifiers', preset)

/** The presets this checkout actually has generated verifiers for. */
export const availablePresets = (): CircuitPreset[] => CIRCUIT_PRESETS.filter((preset) => existsSync(verifierDir(preset)))

/**
 * The preset to deploy against.
 *
 * `CRISP_PRESET` wins. Otherwise this resolves only when the choice is unambiguous — exactly one
 * preset has generated verifiers — and throws when more than one does.
 *
 * There is deliberately no fallback to insecure-512. A verifier encodes its circuit's verification
 * key, so deploying the wrong one produces a round that rejects every ballot, and defaulting
 * quietly to the insecure parameters is the specific mistake worth making impossible.
 */
export const activePreset = (): CircuitPreset => {
  const preset = process.env.CRISP_PRESET

  if (preset !== undefined) {
    if (!CIRCUIT_PRESETS.includes(preset as CircuitPreset)) {
      throw new Error(`CRISP_PRESET must be one of ${CIRCUIT_PRESETS.join(', ')}; got "${preset}".`)
    }

    return preset as CircuitPreset
  }

  const available = availablePresets()
  if (available.length === 1) return available[0]
  if (available.length === 0) {
    throw new Error('No generated verifiers found under contracts/verifiers/. Run scripts/compile_circuits.sh.')
  }

  throw new Error(`Set CRISP_PRESET: verifiers exist for ${available.join(' and ')}, so the choice is not implied.`)
}

/**
 * Fully qualified names for one verifier stack.
 *
 * `libraryKeys` carry hardhat's `project/` prefix, which library linking requires and plain factory
 * lookups reject, so the two forms are returned separately rather than derived at each call site.
 */
export const verifierNames = (variant: VerifierVariant, preset: CircuitPreset = activePreset()) => {
  const source = `contracts/verifiers/${preset}/${VERIFIER_FILES[variant]}`

  return {
    preset,
    source,
    honkVerifier: `${source}:HonkVerifier`,
    zkTranscriptLib: `${source}:ZKTranscriptLib`,
    relationsLib: `${source}:RelationsLib`,
    libraryKeys: {
      zkTranscriptLib: `project/${source}:ZKTranscriptLib`,
      relationsLib: `project/${source}:RelationsLib`,
    },
  }
}
