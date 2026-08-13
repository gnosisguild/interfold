// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { describe, it, expect, beforeAll, beforeEach, afterEach, afterAll, vi } from 'vitest'
import { Vote } from '../src/types'
import { MAX_MSG_NON_ZERO_COEFFS, MAX_VOTE_OPTIONS, SIGNATURE_MESSAGE_HASH, SIGNATURE_MESSAGE } from '../src/constants'
import { getZeroVote } from '../src/utils'
import { decodeTally, verifyProof, encodeVote, generateBFVKeys, encryptVote, decryptVote, destroyBBApi } from '../src/vote'
import { publicKeyToAddress, signMessage } from 'viem/accounts'
import { Hex, recoverPublicKey } from 'viem'
import { CRISP_SERVER_URL, ECDSA_PRIVATE_KEY, SLOT_ADDRESS } from './constants'
import { CrispSDK } from '../src/sdk'
import { generateTestLeaves } from './helpers'

describe('Vote', () => {
  let vote: Vote
  let signature: Hex
  let balance: bigint
  let address: string
  let leaves: bigint[]
  let publicKey: Uint8Array
  let secretKey: Uint8Array
  let previousCiphertext: Uint8Array
  let e3Id: bigint
  let sdk: CrispSDK

  const zeroVote = getZeroVote(2)

  const mockGetPreviousCiphertextResponse = () =>
    ({
      ok: true,
      status: 200,
      json: async () => ({ ciphertext: previousCiphertext }),
    }) as Response

  const mockPreviousCiphertextNotFoundResponse = () => ({ ok: false, status: 404 }) as Response

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  afterAll(() => {
    destroyBBApi()
  })

  beforeAll(async () => {
    vote = [10, 0, 0]
    signature = await signMessage({ message: SIGNATURE_MESSAGE, privateKey: ECDSA_PRIVATE_KEY })
    balance = 10n
    address = publicKeyToAddress(await recoverPublicKey({ hash: SIGNATURE_MESSAGE_HASH, signature }))
    leaves = generateTestLeaves([
      { address, balance },
      { address: SLOT_ADDRESS, balance },
    ])
    const keys = generateBFVKeys()
    publicKey = keys.publicKey
    secretKey = keys.secretKey
    previousCiphertext = encryptVote(zeroVote, publicKey)
    e3Id = (1n << 200n) + 7n
    sdk = new CrispSDK(CRISP_SERVER_URL)
  })

  describe('decodeTally', () => {
    it('Should decode an encoded tally into its decimal representation', () => {
      const expected: Vote = [10000000000, 30000000000]
      const encoded = encodeVote(expected)
      const decoded = decodeTally(encoded, 2)

      expect(decoded[0]).toBe(BigInt(expected[0]))
      expect(decoded[1]).toBe(BigInt(expected[1]))
    })

    it('Should decode totals above Number.MAX_SAFE_INTEGER without losing precision', () => {
      // After aggregation a coefficient is a ballot count, not a bit. This models 4096
      // ballots landing on the top coefficient of option 0 and one on its bottom coefficient,
      // giving a total that a double cannot represent exactly.
      const coefficients = new Array(MAX_MSG_NON_ZERO_COEFFS).fill(0)
      coefficients[0] = 4096
      coefficients[49] = 1

      const decoded = decodeTally(coefficients, 2)

      expect(decoded[0]).toBe((1n << 61n) + 1n)
      expect(decoded[0] > BigInt(Number.MAX_SAFE_INTEGER)).toBe(true)
    })

    it('Should reject a tally shorter than the payload region', () => {
      const tooShort = new Array(MAX_MSG_NON_ZERO_COEFFS - 1).fill(0)

      expect(() => decodeTally(tooShort, 2)).toThrow('is less than MAX_MSG_NON_ZERO_COEFFS')
    })

    it('Should reject fewer than two choices', () => {
      const coefficients = new Array(MAX_MSG_NON_ZERO_COEFFS).fill(0)

      // `CRISPProgram.validate` reverts below 2, so a one-option tally cannot exist on-chain.
      expect(() => decodeTally(coefficients, 1)).toThrow('must be an integer of at least 2')
      expect(() => decodeTally(coefficients, 0)).toThrow('must be an integer of at least 2')
      expect(() => decodeTally(coefficients, -1)).toThrow('must be an integer of at least 2')
      // The lower boundary itself stays decodable.
      expect(decodeTally(coefficients, 2)).toHaveLength(2)
    })

    it('Should reject a non-integer number of choices', () => {
      const coefficients = new Array(MAX_MSG_NON_ZERO_COEFFS).fill(0)

      // A fraction silently decoded `ceil(numChoices)` segments; NaN passed both bound
      // checks and returned an empty tally.
      expect(() => decodeTally(coefficients, 2.5)).toThrow('must be an integer of at least 2')
      expect(() => decodeTally(coefficients, Number.NaN)).toThrow('must be an integer of at least 2')
      expect(() => decodeTally(coefficients, Number.POSITIVE_INFINITY)).toThrow('must be an integer of at least 2')
    })

    it('Should reject more choices than the circuit allows', () => {
      const coefficients = new Array(MAX_MSG_NON_ZERO_COEFFS).fill(0)

      expect(() => decodeTally(coefficients, MAX_VOTE_OPTIONS + 1)).toThrow('exceeds MAX_VOTE_OPTIONS')
      // The boundary itself stays decodable.
      expect(decodeTally(coefficients, MAX_VOTE_OPTIONS)).toHaveLength(MAX_VOTE_OPTIONS)
    })
  })

  describe('encodeVote', () => {
    it('Should fail when the number of choices is less than 2', () => {
      expect(() => encodeVote([10])).toThrow('Vote must have at least two choices')
      expect(() => encodeVote([])).toThrow('Vote must have at least two choices')
    })

    it('Should fail when the number of choices exceeds the circuit maximum', () => {
      expect(() => encodeVote(new Array(MAX_VOTE_OPTIONS + 1).fill(0))).toThrow('exceeds MAX_VOTE_OPTIONS')
      // The boundary itself still encodes and round-trips.
      const encoded = encodeVote(new Array(MAX_VOTE_OPTIONS).fill(1))
      expect(decodeTally(encoded, MAX_VOTE_OPTIONS)).toEqual(new Array(MAX_VOTE_OPTIONS).fill(1n))
    })

    it('Should encode votes correctly with 2 choices', () => {
      const encoded = encodeVote([10, 2])
      const decoded = decodeTally(encoded, 2)

      expect(decoded[0]).toBe(10n)
      expect(decoded[1]).toBe(2n)
    })

    it('Should encode zero votes correctly', () => {
      const encoded = encodeVote([0, 5])
      const decoded = decodeTally(encoded, 2)

      expect(decoded[0]).toBe(0n)
      expect(decoded[1]).toBe(5n)
    })

    it('Should only contain binary digits (0 or 1)', () => {
      const encoded = encodeVote([255, 128])

      expect(Array.from(encoded).every((b) => b === 0 || b === 1)).toBe(true)
    })

    it('Should encode votes correctly with 3 choices', () => {
      const encoded = encodeVote([10, 2, 3])
      const decoded = decodeTally(encoded, 3)

      expect(decoded[0]).toBe(10n)
      expect(decoded[1]).toBe(2n)
      expect(decoded[2]).toBe(3n)
    })

    it('Should encode votes correctly with 5 choices', () => {
      const encoded = encodeVote([100, 50, 25, 10, 5])
      const decoded = decodeTally(encoded, 5)

      expect(decoded[0]).toBe(100n)
      expect(decoded[1]).toBe(50n)
      expect(decoded[2]).toBe(25n)
      expect(decoded[3]).toBe(10n)
      expect(decoded[4]).toBe(5n)
    })

    it('Should zero-pad unused slots in the first MAX_MSG_NON_ZERO_COEFFS coeffs for 3 choices', () => {
      const encoded = encodeVote([1, 1, 1])
      const decoded = decodeTally(encoded, 3)

      expect(decoded[0]).toBe(1n)
      expect(decoded[1]).toBe(1n)
      expect(decoded[2]).toBe(1n)

      const segmentSize = Math.floor(MAX_MSG_NON_ZERO_COEFFS / 3)
      expect(encoded.slice(segmentSize * 3, MAX_MSG_NON_ZERO_COEFFS).every((b) => b === 0)).toBe(true)
    })
  })

  describe('generateVoteProof', () => {
    it('Should generate a valid vote proof', { timeout: 300000 }, async () => {
      vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockPreviousCiphertextNotFoundResponse())

      const proof = await sdk.generateVoteProof({
        vote,
        publicKey,
        signature,
        merkleLeaves: leaves,
        balance,
        messageHash: SIGNATURE_MESSAGE_HASH,
        slotAddress: SLOT_ADDRESS,
        e3Id,
      })

      expect(proof).toBeDefined()
      expect(proof.proof).toBeDefined()
      expect(proof.publicInputs).toBeDefined()
      expect(proof.encryptedVote).toBeDefined()

      const decryptedVote = decryptVote(proof.encryptedVote, secretKey, vote.length)

      expect(decryptedVote).toEqual(vote.map(BigInt))

      const isValid = await verifyProof(proof)

      expect(isValid).toBe(true)
    })
  })

  describe('generateMaskVoteProof', () => {
    it('Should generate a valid mask vote proof when there are no votes in the slot', { timeout: 300000 }, async () => {
      vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockPreviousCiphertextNotFoundResponse())

      const proof = await sdk.generateMaskVoteProof({
        balance,
        slotAddress: SLOT_ADDRESS,
        publicKey,
        merkleLeaves: leaves,
        e3Id: 0n,
        numOptions: 2,
      })

      expect(proof).toBeDefined()
      expect(proof.proof).toBeDefined()
      expect(proof.publicInputs).toBeDefined()
      expect(proof.encryptedVote).toBeDefined()

      const decryptedVote = decryptVote(proof.encryptedVote, secretKey, 2)

      expect(decryptedVote).toEqual(zeroVote.map(BigInt))

      const isValid = await verifyProof(proof)

      expect(isValid).toBe(true)
    })

    it('Should generate a valid mask vote proof when there is a previous vote in the slot', { timeout: 300000 }, async () => {
      vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockGetPreviousCiphertextResponse())

      const proof = await sdk.generateMaskVoteProof({
        balance,
        slotAddress: SLOT_ADDRESS,
        publicKey,
        merkleLeaves: leaves,
        e3Id: 0n,
        numOptions: 2,
      })

      expect(proof).toBeDefined()
      expect(proof.proof).toBeDefined()
      expect(proof.publicInputs).toBeDefined()

      const decryptedVote = decryptVote(previousCiphertext, secretKey, 2)

      expect(decryptedVote).toEqual(zeroVote.map(BigInt))

      const isValid = await verifyProof(proof)

      expect(isValid).toBe(true)
    })
  })
})
