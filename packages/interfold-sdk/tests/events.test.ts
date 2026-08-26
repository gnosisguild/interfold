// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { CiphernodeRegistryOwnable__factory, IRandomnessProvider__factory } from '@interfold/contracts/types'
import type { Log, PublicClient } from 'viem'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { EventListener } from '../src/events/event-listener'
import { RandomnessProviderEventType, RegistryEventType, type InterfoldEvent } from '../src/events/types'

afterEach(() => {
  vi.restoreAllMocks()
})

describe('RegistryEventType', () => {
  it('uses the finalized-committee event name exposed by the registry ABI', () => {
    const registryEventNames = CiphernodeRegistryOwnable__factory.abi.filter((item) => item.type === 'event').map((item) => item.name)

    expect(RegistryEventType.COMMITTEE_FINALIZED).toBe('SortitionCommitteeFinalized')
    expect(registryEventNames).toContain(RegistryEventType.COMMITTEE_FINALIZED)
  })

  it('exposes the asynchronous randomness lifecycle events', () => {
    const registryEventNames = CiphernodeRegistryOwnable__factory.abi.filter((item) => item.type === 'event').map((item) => item.name)

    expect(RegistryEventType.COMMITTEE_RANDOMNESS_REQUESTED).toBe('CommitteeRandomnessRequested')
    expect(registryEventNames).toContain(RegistryEventType.COMMITTEE_RANDOMNESS_REQUESTED)
    expect(RegistryEventType.RANDOMNESS_CIRCUIT_BREAKER_TRIPPED).toBe('RandomnessCircuitBreakerTripped')
    expect(registryEventNames).toContain(RegistryEventType.RANDOMNESS_CIRCUIT_BREAKER_TRIPPED)
  })

  it('watches fulfillment on the request-bound provider', async () => {
    const provider = '0x0000000000000000000000000000000000000004' as const
    const unwatch = vi.fn()
    const watchContractEvent = vi.fn().mockReturnValue(unwatch)
    const callback = vi.fn()
    const listener = new EventListener({
      publicClient: { watchContractEvent } as unknown as PublicClient,
      contracts: {
        interfold: '0x0000000000000000000000000000000000000001',
        ciphernodeRegistry: '0x0000000000000000000000000000000000000002',
        feeToken: '0x0000000000000000000000000000000000000003',
      },
    })

    await listener.onRandomnessProviderEvent(provider, RandomnessProviderEventType.RANDOMNESS_FULFILLED, callback)
    const options = watchContractEvent.mock.calls[0]?.[0]
    expect(options.address).toBe(provider)
    expect(options.abi).toBe(IRandomnessProvider__factory.abi)
    expect(options.eventName).toBe('RandomnessFulfilled')

    const log = {
      args: { requestId: 7n, e3Id: 11n, randomWord: 13n, fulfilledAt: 17n },
      blockNumber: 19n,
      transactionHash: '0x1234',
    } as unknown as Log
    options.onLogs([log])

    expect(callback).toHaveBeenCalledWith(
      expect.objectContaining({
        type: RandomnessProviderEventType.RANDOMNESS_FULFILLED,
        provider,
        data: { requestId: 7n, e3Id: 11n, randomWord: 13n, fulfilledAt: 17n },
        blockNumber: 19n,
      }),
    )

    listener.offRandomnessProviderEvent(provider, RandomnessProviderEventType.RANDOMNESS_FULFILLED, callback)
    expect(unwatch).toHaveBeenCalledOnce()
  })

  it('queries historical provider fulfillment', async () => {
    const provider = '0x0000000000000000000000000000000000000004' as const
    const getContractEvents = vi.fn().mockResolvedValue([])
    const listener = new EventListener({
      publicClient: { getContractEvents } as unknown as PublicClient,
      contracts: {
        interfold: '0x0000000000000000000000000000000000000001',
        ciphernodeRegistry: '0x0000000000000000000000000000000000000002',
        feeToken: '0x0000000000000000000000000000000000000003',
      },
    })

    await listener.getHistoricalRandomnessProviderEvents(provider, RandomnessProviderEventType.RANDOMNESS_FULFILLED, 10n, 20n)

    expect(getContractEvents).toHaveBeenCalledWith({
      address: provider,
      abi: IRandomnessProvider__factory.abi,
      eventName: RandomnessProviderEventType.RANDOMNESS_FULFILLED,
      fromBlock: 10n,
      toBlock: 20n,
    })
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
