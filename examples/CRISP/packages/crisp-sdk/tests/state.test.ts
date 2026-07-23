// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'

import { getRoundDetails, getRoundTokenDetails } from '../src/state'
import { CRISP_SERVER_URL } from './constants'
import { CRISP_SERVER_STATE_LITE_ENDPOINT } from '../src/constants'
import { zeroAddress } from 'viem'
import { CreditMode } from '../src/types'
import type { E3StateLiteResponse } from '../src/types'

describe('State', () => {
  const mockStateLiteResponse: E3StateLiteResponse = {
    id: 0,
    chain_id: 11155111,
    interfold_address: '0x1234567890123456789012345678901234567890',
    status: 'active',
    vote_count: 10,
    start_time: 1000000,
    end_time: 1086400,
    start_block: 12345,
    committee_public_key: [1, 2, 3],
    emojis: ['👍', '👎'],
    token_address: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
    balance_threshold: '1000',
    num_options: '2',
    requester: '0x9876543210987654321098765432109876543210',
    credit_mode: CreditMode.CONSTANT,
    credits: null,
  }

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  describe('getRoundDetails', () => {
    it('should get the state for a given e3Id from the CRISP server', async () => {
      const mockResponse = mockStateLiteResponse

      const mockFetchResponse = {
        ok: true,
        json: async () => mockResponse,
      } as Response

      vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockFetchResponse)

      const state = await getRoundDetails(CRISP_SERVER_URL, 0)

      expect(state).toBeDefined()
      expect(state.e3Id).toBe(0n)
      expect(state.chainId).toBe(11155111n)
      expect(state.interfoldAddress).toBe('0x1234567890123456789012345678901234567890')
      expect(state.status).toBe('active')
      expect(state.voteCount).toBe(10n)
      expect(state.startTime).toBe(1000000n)
      expect(state.endTime).toBe(1086400n)
      expect(state.startBlock).toBe(12345n)
      expect(state.committeePublicKey).toEqual(new Uint8Array([1, 2, 3]))
      expect(state.emojis).toEqual(['👍', '👎'])
      expect(state.tokenAddress).toBe('0xabcdefabcdefabcdefabcdefabcdefabcdefabcd')
      expect(state.balanceThreshold).toBe(1000n)
      expect(state.numOptions).toBe(2n)
      expect(state.requester).toBe('0x9876543210987654321098765432109876543210')
      expect(state.creditMode).toBe(CreditMode.CONSTANT)
      expect(state.credits).toBeUndefined()

      expect(fetch).toHaveBeenCalledWith(
        `${CRISP_SERVER_URL}/${CRISP_SERVER_STATE_LITE_ENDPOINT}`,
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ round_id: 0 }),
        }),
      )
    })

    it('should convert custom credits to a bigint', async () => {
      const mockFetchResponse = {
        ok: true,
        json: async () => ({ ...mockStateLiteResponse, credit_mode: CreditMode.CUSTOM, credits: '500' }),
      } as Response

      vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockFetchResponse)

      const state = await getRoundDetails(CRISP_SERVER_URL, 0)

      expect(state.creditMode).toBe(CreditMode.CUSTOM)
      expect(state.credits).toBe(500n)
    })
  })

  describe('getTokenDetails', () => {
    it('should return the details of the token for a given e3Id from the CRISP server', async () => {
      const mockResponse = mockStateLiteResponse

      const mockFetchResponse = {
        ok: true,
        json: async () => mockResponse,
      } as Response

      vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockFetchResponse)

      const tokenDetails = await getRoundTokenDetails(CRISP_SERVER_URL, 0)

      expect(tokenDetails.tokenAddress).not.toBe(zeroAddress)
      expect(tokenDetails.tokenAddress).toBe('0xabcdefabcdefabcdefabcdefabcdefabcdefabcd')
      expect(tokenDetails.threshold).toBeGreaterThan(0)
      expect(tokenDetails.threshold).toBe(1000n)
      expect(tokenDetails.snapshotBlock).toBeGreaterThan(0)
      expect(tokenDetails.snapshotBlock).toBe(12345n)
    })
  })
})
