// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { describe, expect, it } from 'vitest'
import {
  decodePlaintextOutput,
  decodePlaintextOutputBigInt,
  encodeBfvParams,
  encodeComputeProviderParams,
  encodeCustomParams,
  formatBigInt,
  isValidAddress,
  isValidHash,
  parseBigInt,
} from '../src/utils'

describe('SDK Utilities', () => {
  describe('decodePlaintextOutput & decodePlaintextOutputBigInt', () => {
    it('should decode little-endian u64 bytes correctly', () => {
      // 42 in little-endian 64-bit hex: 0x2a00000000000000
      const hex = '0x2a00000000000000'
      expect(decodePlaintextOutput(hex)).to.equal(42)
      expect(decodePlaintextOutputBigInt(hex)).to.equal(42n)
    })

    it('should handle large u64 values without losing BigInt precision', () => {
      // 18,446,744,073,709,551,615 (u64 max) = 0xffffffffffffffff
      const hex = '0xffffffffffffffff'
      expect(decodePlaintextOutputBigInt(hex)).to.equal(18446744073709551615n)
    })

    it('should handle odd-length hex input by zero-padding', () => {
      // Odd-length hex e.g. 0x2a0000000000000 (15 chars) -> zero padded to 16 chars
      const oddHex = '0x2a0000000000000'
      const decoded = decodePlaintextOutputBigInt(oddHex)
      expect(decoded).to.not.be.null
    })

    it('should return null for short plaintext outputs (< 8 bytes)', () => {
      expect(decodePlaintextOutput('0x1234')).to.be.null
      expect(decodePlaintextOutputBigInt('0x1234')).to.be.null
    })

    it('should return null for empty string or invalid hex', () => {
      expect(decodePlaintextOutput('')).to.be.null
      expect(decodePlaintextOutput('invalid')).to.be.null
    })
  })

  describe('Address & Hash Validation', () => {
    it('should validate valid Ethereum addresses', () => {
      expect(isValidAddress('0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045')).to.be.true
      expect(isValidAddress('0x0000000000000000000000000000000000000000')).to.be.true
      expect(isValidAddress('invalid-address')).to.be.false
      expect(isValidAddress('0x1234')).to.be.false
    })

    it('should validate valid bytes32 hashes', () => {
      const validHash = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80'
      expect(isValidHash(validHash)).to.be.true
      expect(isValidHash('0x1234')).to.be.false
    })
  })

  describe('BFV & Provider Parameter Encoding', () => {
    it('should encode BFV parameters correctly', () => {
      const encoded = encodeBfvParams({
        degree: 512,
        plaintextModulus: 65537,
        moduli: [1152921504606846977n],
        error1Variance: '3.2',
      })
      expect(encoded).to.be.a('string')
      expect(encoded.startsWith('0x')).to.be.true
    })

    it('should encode compute provider params and mock mode', () => {
      const mockParams = encodeComputeProviderParams({ name: 'risc0', parallel: false, batch_size: 2 }, true)
      expect(mockParams).to.equal(`0x${'00'.repeat(32)}`)

      const realParams = encodeComputeProviderParams({ name: 'risc0', parallel: false, batch_size: 2 }, false)
      expect(realParams.startsWith('0x')).to.be.true
    })

    it('should encode custom parameters as hex', () => {
      const encoded = encodeCustomParams({ key: 'value' })
      expect(encoded.startsWith('0x')).to.be.true
    })
  })

  describe('BigInt Formatting', () => {
    it('should format and parse BigInt values', () => {
      expect(formatBigInt(100n)).to.equal('100')
      expect(parseBigInt('100')).to.equal(100n)
    })
  })
})
