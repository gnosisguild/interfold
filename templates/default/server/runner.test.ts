// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { callFheRunner } from './runner'

describe('callFheRunner', () => {
  beforeEach(() => {
    delete process.env.PROGRAM_RUNNER_URL
    delete process.env.CALLBACK_URL
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('submits without authorization and does not log the payload', async () => {
    process.env.PROGRAM_RUNNER_URL = 'http://127.0.0.1:13151'
    process.env.CALLBACK_URL = 'http://127.0.0.1:8080'

    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ status: 'processing', e3_id: 7 }),
    })
    vi.stubGlobal('fetch', fetchMock)
    const logMock = vi.spyOn(console, 'log').mockImplementation(() => undefined)

    await callFheRunner(7n, '0x0102', [['0x03', 0]])

    expect(fetchMock).toHaveBeenCalledOnce()
    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:13151/run_compute',
      expect.objectContaining({
        headers: {
          'Content-Type': 'application/json',
        },
      }),
    )
    expect(JSON.stringify(logMock.mock.calls)).not.toContain('0x0102')
    expect(JSON.stringify(logMock.mock.calls)).not.toContain('0x03')
  })
})
