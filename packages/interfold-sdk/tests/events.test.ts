// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { CiphernodeRegistryOwnable__factory } from '@interfold/contracts/types'
import type { Log, PublicClient } from 'viem'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { EventListener } from '../src/events/event-listener'
import { RegistryEventType, type InterfoldEvent } from '../src/events/types'

afterEach(() => {
  vi.restoreAllMocks()
})

describe('RegistryEventType', () => {
  it('uses the finalized-committee event name exposed by the registry ABI', () => {
    const registryEventNames = CiphernodeRegistryOwnable__factory.abi.filter((item) => item.type === 'event').map((item) => item.name)

    expect(RegistryEventType.COMMITTEE_FINALIZED).toBe('SortitionCommitteeFinalized')
    expect(registryEventNames).toContain(RegistryEventType.COMMITTEE_FINALIZED)
  })

  it('handles asynchronous event callback failures', async () => {
    const error = new Error('invalid committee key')
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const listener = new EventListener({
      publicClient: {} as PublicClient,
      contracts: {
        interfold: '0x0000000000000000000000000000000000000001',
        ciphernodeRegistry: '0x0000000000000000000000000000000000000002',
        feeToken: '0x0000000000000000000000000000000000000003',
      },
    })
    const event: InterfoldEvent<RegistryEventType.COMMITTEE_PUBLISHED> = {
      type: RegistryEventType.COMMITTEE_PUBLISHED,
      data: {
        e3Id: 1n,
        nodes: [],
        publicKey: '0x',
        pkCommitment: `0x${'00'.repeat(32)}`,
        proof: '0x',
      },
      log: {} as Log,
      timestamp: new Date(),
      blockNumber: 1n,
      transactionHash: '0x',
    }

    listener.on(RegistryEventType.COMMITTEE_PUBLISHED, async () => {
      throw error
    })
    listener.emit(event)
    await Promise.resolve()

    expect(consoleError).toHaveBeenCalledWith(`Error in event callback for ${RegistryEventType.COMMITTEE_PUBLISHED}:`, error)
  })

  it('preserves the request-time ticket price', () => {
    const listener = new EventListener({
      publicClient: {} as PublicClient,
      contracts: {
        interfold: '0x0000000000000000000000000000000000000001',
        ciphernodeRegistry: '0x0000000000000000000000000000000000000002',
        feeToken: '0x0000000000000000000000000000000000000003',
      },
    })
    const callback = vi.fn()
    const event: InterfoldEvent<RegistryEventType.COMMITTEE_REQUESTED> = {
      type: RegistryEventType.COMMITTEE_REQUESTED,
      data: {
        e3Id: 1n,
        entropyBlock: 2n,
        threshold: [2n, 3n],
        requestBlock: 4n,
        committeeDeadline: 5n,
        ticketPrice: 10_000_000n,
      },
      log: {} as Log,
      timestamp: new Date(),
      blockNumber: 4n,
      transactionHash: '0x',
    }

    listener.on(RegistryEventType.COMMITTEE_REQUESTED, callback)
    listener.emit(event)

    expect(callback).toHaveBeenCalledWith(event)
    expect(event.data.ticketPrice).toBe(10_000_000n)
  })
})
