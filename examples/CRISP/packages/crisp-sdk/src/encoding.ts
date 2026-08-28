// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

/**
 * Vote encoding and BFV encryption for the CRISP voting protocol.
 *
 * Encodes vote choices (numbers per option) into polynomial coefficient arrays
 * suitable for BFV homomorphic encryption. Each choice is represented as a
 * segment of binary digits within the first MAX_MSG_NON_ZERO_COEFFS coeffs, then
 * zero-padded to the BFV polynomial degree. Supports
 * encoding, encryption, decryption, and tally decoding.
 */

import { ZKInputsGenerator } from '@crisp-e3/zk-inputs'
import { registeredPreset, type CircuitPreset } from './circuits'
import { toBinary, numberArrayToBigInt64Array, decodeBytesToBigInts, getMaxVoteValue } from './utils'
import { MAX_MSG_NON_ZERO_COEFFS, MAX_VOTE_OPTIONS } from './constants'
import { hexToBytes } from 'viem'
import type { Hex } from 'viem'
import type { TallyResult, Vote } from './types'

let _zkInputsGenerator: InstanceType<typeof ZKInputsGenerator> | null = null
let _zkInputsGeneratorPreset: CircuitPreset | 'default' | null = null
let _zkInputsGeneratorPresetOverride: CircuitPreset | null = null

/** Set or clear the BFV preset override for contexts that do not share the registered bundle. */
export const setZkInputsGeneratorPreset = (preset: CircuitPreset | null): void => {
  if (_zkInputsGeneratorPresetOverride !== preset) {
    _zkInputsGenerator = null
    _zkInputsGeneratorPreset = null
    _zkInputsGeneratorPresetOverride = preset
  }
}

/**
 * Returns the singleton ZK inputs generator instance for the registered BFV preset.
 */
export const getZkInputsGenerator = () => {
  const preset = _zkInputsGeneratorPresetOverride ?? registeredPreset()
  const targetPreset = preset ?? 'default'

  if (!_zkInputsGenerator || _zkInputsGeneratorPreset !== targetPreset) {
    _zkInputsGenerator = preset ? ZKInputsGenerator.fromPreset(preset) : ZKInputsGenerator.withDefaults()
    _zkInputsGeneratorPreset = targetPreset
  }

  return _zkInputsGenerator
}

/**
 * Encodes vote choices into a polynomial coefficient array for BFV encryption.
 * Each choice occupies floor(MAX_MSG_NON_ZERO_COEFFS / n) binary coefficients;
 * remaining slots in the first MAX_MSG_NON_ZERO_COEFFS coeffs are zero; then
 * the vector is padded to the BFV degree.
 *
 * @param vote - Array of numeric values per choice (e.g. [10, 5] for 2 options)
 * @returns Array of 0s and 1s representing coefficients
 * @throws If vote has fewer than 2 choices, any value exceeds max for its segment, or degree is too small
 */
export const encodeVote = (vote: Vote): number[] => {
  const numChoices = vote.length

  if (numChoices < 2) {
    throw new Error('Vote must have at least two choices')
  }

  // The Noir circuit asserts num_options <= MAX_OPTIONS, so a vote beyond this can never
  // produce a valid proof. Reject it here rather than encoding an unprovable vote.
  if (numChoices > MAX_VOTE_OPTIONS) {
    throw new Error(`Number of choices (${numChoices}) exceeds MAX_VOTE_OPTIONS (${MAX_VOTE_OPTIONS})`)
  }

  const bfvParams = getZkInputsGenerator().getBFVParams()
  const degree = bfvParams.degree
  if (degree < MAX_MSG_NON_ZERO_COEFFS) {
    throw new Error(`BFV degree (${degree}) must be at least MAX_MSG_NON_ZERO_COEFFS (${MAX_MSG_NON_ZERO_COEFFS})`)
  }

  const segmentSize = Math.floor(MAX_MSG_NON_ZERO_COEFFS / numChoices)
  const maxValue = getMaxVoteValue(numChoices)
  const voteArray: number[] = []

  for (let choiceIdx = 0; choiceIdx < numChoices; choiceIdx += 1) {
    const value = vote[choiceIdx]

    if (value > maxValue) {
      throw new Error(`Vote value for choice ${choiceIdx} exceeds maximum (${maxValue})`)
    }

    const binary = toBinary(value).split('')

    for (let i = 0; i < segmentSize; i += 1) {
      const offset = segmentSize - binary.length
      voteArray.push(i < offset ? 0 : parseInt(binary[i - offset], 10))
    }
  }

  const msgCoeffsUsed = segmentSize * numChoices
  for (let i = msgCoeffsUsed; i < MAX_MSG_NON_ZERO_COEFFS; i += 1) {
    voteArray.push(0)
  }

  for (let i = 0; i < degree - MAX_MSG_NON_ZERO_COEFFS; i += 1) {
    voteArray.push(0)
  }

  return voteArray
}

/**
 * Encrypts an encoded vote using BFV homomorphic encryption.
 *
 * @param vote - Vote choices to encrypt
 * @param publicKey - BFV public key
 * @returns Encrypted ciphertext
 */
export const encryptVote = (vote: Vote, publicKey: Uint8Array): Uint8Array => {
  const encodedVote = encodeVote(vote)

  return getZkInputsGenerator().encryptVote(publicKey, numberArrayToBigInt64Array(encodedVote))
}

/**
 * Decodes raw tally bytes (or coefficients) into a total per choice.
 * Expects the same segment layout as used in encodeVote.
 *
 * Mirrors `crisp_utils::decode_tally` (Rust) and `CRISPProgram.decodeTally` (Solidity):
 * only the first MAX_MSG_NON_ZERO_COEFFS coefficients carry the payload, split into
 * `floor(MAX_MSG_NON_ZERO_COEFFS / numChoices)` binary coefficients per choice, MSB first.
 *
 * @param tallyBytes - Hex string, or the polynomial coefficients from tally/decryption
 * @param numChoices - Number of vote options: an integer from 2 to MAX_VOTE_OPTIONS
 * @returns One total per choice
 * @throws If numChoices is outside 2..MAX_VOTE_OPTIONS or not an integer, or there are fewer
 *         coefficients than the payload region
 */
export const decodeTally = (tallyBytes: string | number[] | bigint[], numChoices: number): TallyResult => {
  // `CRISPProgram.validate` rejects a round outside 2..MAX_VOTE_OPTIONS, and `encodeVote` refuses
  // to encode fewer than two choices, so no tally in that range can exist. `Number.isInteger` also
  // screens out NaN, Infinity, and fractions: a fractional count silently returns `ceil(numChoices)`
  // segments, and NaN passes both bound checks to return an empty tally.
  if (!Number.isInteger(numChoices) || numChoices < 2) {
    throw new Error(`Number of choices (${numChoices}) must be an integer of at least 2`)
  }

  // Rounds cannot exceed MAX_VOTE_OPTIONS (the circuit's MAX_OPTIONS), so a larger count
  // is a caller error rather than a tally to decode.
  if (numChoices > MAX_VOTE_OPTIONS) {
    throw new Error(`Number of choices (${numChoices}) exceeds MAX_VOTE_OPTIONS (${MAX_VOTE_OPTIONS})`)
  }

  let coefficients: bigint[]
  if (typeof tallyBytes === 'string') {
    const hexString = tallyBytes.startsWith('0x') ? tallyBytes : `0x${tallyBytes}`
    coefficients = decodeBytesToBigInts(hexToBytes(hexString as Hex))
  } else {
    coefficients = (tallyBytes as Array<number | bigint>).map(BigInt)
  }

  if (coefficients.length < MAX_MSG_NON_ZERO_COEFFS) {
    throw new Error(`decoded coefficient count (${coefficients.length}) is less than MAX_MSG_NON_ZERO_COEFFS (${MAX_MSG_NON_ZERO_COEFFS})`)
  }

  const segmentSize = Math.floor(MAX_MSG_NON_ZERO_COEFFS / numChoices)
  const results: TallyResult = []

  for (let choiceIdx = 0; choiceIdx < numChoices; choiceIdx++) {
    const segmentStart = choiceIdx * segmentSize

    let value = 0n
    for (let i = 0; i < segmentSize; i++) {
      value += coefficients[segmentStart + i] << BigInt(segmentSize - 1 - i)
    }

    results.push(value)
  }

  return results
}

/**
 * Decrypts a BFV-encrypted vote and decodes it to vote values.
 *
 * @param ciphertext - Encrypted vote
 * @param secretKey - BFV secret key
 * @param numChoices - Number of vote options
 * @returns One total per choice
 */
export const decryptVote = (ciphertext: Uint8Array, secretKey: Uint8Array, numChoices: number): TallyResult => {
  const decryptedVote = getZkInputsGenerator().decryptVote(secretKey, ciphertext)

  return decodeTally(
    Array.from(decryptedVote, (value) => BigInt(value)),
    numChoices,
  )
}

/**
 * Generates a BFV keypair for vote encryption and decryption.
 *
 * @returns Object with secretKey and publicKey as Uint8Arrays
 */
export const generateBFVKeys = (): { secretKey: Uint8Array; publicKey: Uint8Array } => {
  return getZkInputsGenerator().generateKeys()
}
