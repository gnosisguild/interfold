// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Network-wide operator stats for the public pulse strip.

import { erc20Abi, type Address } from 'viem'
import { useEffect, useRef, useState } from 'react'
import { CONTRACTS, bondingRegistryAbi, ciphernodeRegistryAbi, publicClient } from './chain'

// Node counts and total bonded move slowly — poll at a fraction of the E3 rate
// so the strip does not add load to rate-limited free-tier RPCs.
const POLL_MS = 60_000

const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000' as Address

export type NetworkStats = {
  /** Ciphernodes currently in the registry tree. */
  registeredNodes: bigint
  /** Operators active (bonded + ticketed) right now. */
  activeOperators: bigint
  /** Total ciphernode bond collateral held by the bonding registry. */
  totalBonded: bigint
  bondSymbol: string
  bondDecimals: number
}

// Bond-token metadata never changes for a deployment — resolved on the first
// successful fetch, then reused so later ticks cost a single multicall.
let tokenMeta: { symbol: string; decimals: number } | null = null

export async function fetchNetworkStats(): Promise<NetworkStats> {
  // eligibilityAt keys its checkpoint lookup by timestamp; "now" returns the
  // latest value. The operator argument only affects the per-operator flag,
  // which we ignore, so the zero address works for the global count.
  const nowSec = BigInt(Math.floor(Date.now() / 1000))
  // Multicall keeps this to one eth_call per tick (two on the first), so the
  // strip cannot crowd out the other fetchers on batch-capped RPC providers.
  const [registeredNodes, [, activeOperators], totalBonded, bondToken] = (await (publicClient.multicall as any)({
    allowFailure: false,
    contracts: [
      { address: CONTRACTS.CiphernodeRegistry, abi: ciphernodeRegistryAbi, functionName: 'numCiphernodes' },
      { address: CONTRACTS.BondingRegistry, abi: bondingRegistryAbi, functionName: 'eligibilityAt', args: [ZERO_ADDRESS, nowSec] },
      { address: CONTRACTS.BondingRegistry, abi: bondingRegistryAbi, functionName: 'totalCiphernodeBondLiability' },
      { address: CONTRACTS.BondingRegistry, abi: bondingRegistryAbi, functionName: 'getCiphernodeBondToken' },
    ],
  })) as [bigint, [boolean, bigint], bigint, Address]

  if (!tokenMeta) {
    const [symbol, decimals] = (await (publicClient.multicall as any)({
      allowFailure: false,
      contracts: [
        { address: bondToken, abi: erc20Abi, functionName: 'symbol' },
        { address: bondToken, abi: erc20Abi, functionName: 'decimals' },
      ],
    })) as [string, number]
    tokenMeta = { symbol, decimals: Number(decimals) }
  }

  return {
    registeredNodes,
    activeOperators,
    totalBonded,
    bondSymbol: tokenMeta.symbol,
    bondDecimals: tokenMeta.decimals,
  }
}

// Network stats for the pulse strip (null until first load; keeps the last
// value on transient errors, matching useRecentBallots).
export function useNetworkStats(): NetworkStats | null {
  const [stats, setStats] = useState<NetworkStats | null>(null)
  const mounted = useRef(true)

  useEffect(() => {
    mounted.current = true
    let cancelled = false
    let inFlight = false
    const tick = async () => {
      if (inFlight) return
      inFlight = true
      try {
        const next = await fetchNetworkStats()
        if (!cancelled && mounted.current) setStats(next)
      } catch {
        /* keep last value on transient errors */
      } finally {
        inFlight = false
      }
    }
    // Stay out of the mount burst (E3 list getLogs + operator-guide reads) so a
    // rate-limited RPC serves the page-critical fetchers first.
    const first = setTimeout(tick, 2_500)
    const id = setInterval(tick, POLL_MS)
    return () => {
      cancelled = true
      mounted.current = false
      clearTimeout(first)
      clearInterval(id)
    }
  }, [])

  return stats
}
