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

/**
 * The preset to deploy against, from `CRISP_PRESET`.
 *
 * Defaults to insecure-512, which is what the circuits compile to by default and what the tests
 * prove against. A deployment that means to use secure-8192 has to say so.
 */
export const activePreset = (): CircuitPreset => {
  const preset = process.env.CRISP_PRESET

  if (preset === undefined) return 'insecure-512'
  if (!CIRCUIT_PRESETS.includes(preset as CircuitPreset)) {
    throw new Error(`CRISP_PRESET must be one of ${CIRCUIT_PRESETS.join(', ')}; got "${preset}".`)
  }

  return preset as CircuitPreset
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
