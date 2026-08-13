// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'

import { getTreeData } from '../src/token'
import { CRISP_SERVER_URL } from './constants'
import { CRISP_SERVER_TOKEN_TREE_ENDPOINT } from '../src/constants'

const GLOBAL_E3_ID = (1n << 200n) + 7n

describe('Token data fetching', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('should fetch token data from the CRISP server', async () => {
    const mockHashes = ['0x1234', '0x5678', '0x9abc']
    const mockResponse = {
      ok: true,
      json: async () => mockHashes,
    } as Response

    vi.spyOn(global, 'fetch').mockResolvedValueOnce(mockResponse)

    const data = await getTreeData(CRISP_SERVER_URL, GLOBAL_E3_ID)

    expect(data).toHaveLength(3)
    expect(data[0]).toBe(BigInt('0x1234'))
    expect(data[1]).toBe(BigInt('0x5678'))
    expect(data[2]).toBe(BigInt('0x9abc'))
    expect(fetch).toHaveBeenCalledWith(
      `${CRISP_SERVER_URL}/${CRISP_SERVER_TOKEN_TREE_ENDPOINT}`,
      expect.objectContaining({
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ round_id: GLOBAL_E3_ID.toString() }),
      }),
    )
  })
})
