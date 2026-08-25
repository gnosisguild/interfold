// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Fully qualified names for the generated Honk verifiers.
//
// There is one verifier per census mode, and they are NOT preset-specific. `compile_circuits.sh`
// generates them from the fold circuit's verification key, and the fold circuit takes the inner
// key as an input and checks its hash against either preset's constant — so its own structure
// carries no BFV degree and a single verifier accepts proofs from either preset. Building both
// presets produces byte-identical verifier sources, which is the design working rather than a
// coincidence to rely on.
//
// Both files still declare a contract named `HonkVerifier`, so every lookup has to be qualified by
// file. That convention lives here so the deploy script and the tests cannot drift apart.

/** Which census a round establishes eligibility with. */
export type VerifierVariant = 'merkle' | 'onchain'

const VERIFIER_FILES: Record<VerifierVariant, string> = {
  merkle: 'CRISPVerifier.sol',
  onchain: 'CRISPOnchainVerifier.sol',
}

/**
 * Fully qualified names for one verifier stack.
 *
 * `libraryKeys` carry hardhat's `project/` prefix, which library linking requires and plain factory
 * lookups reject, so the two forms are returned separately rather than derived at each call site.
 */
export const verifierNames = (variant: VerifierVariant) => {
  const source = `contracts/verifiers/${VERIFIER_FILES[variant]}`

  return {
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
