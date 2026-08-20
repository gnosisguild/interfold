// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Network pulse — stat card at the top of every view.

import { formatUnits } from 'viem'
import type { NetworkStats } from './lib/network'
import { NETWORK_NAME } from './lib/chain'

// Human-scale token amount, compacted above 10k (96,000 → "96K").
function fmtBonded(value: bigint, decimals: number): string {
  const asNumber = Number(formatUnits(value, decimals))
  if (asNumber >= 10_000) {
    return asNumber.toLocaleString(undefined, { notation: 'compact', maximumFractionDigits: 1 })
  }
  return asNumber.toLocaleString(undefined, { maximumFractionDigits: asNumber >= 1000 ? 0 : 2 })
}

function Tile({ value, label }: { value: string; label: string }) {
  return (
    <div className='netpulse__tile'>
      <div className='netpulse__value'>{value}</div>
      <div className='netpulse__label'>{label}</div>
    </div>
  )
}

export default function Pulse({
  data,
  network,
}: {
  data: { activeNow: number; ballots24h: number; pollsAllTime: number }
  network: NetworkStats | null
}) {
  return (
    <section className='netpulse' aria-label='Network activity'>
      <div className='netpulse__head'>
        <span className='dot-live' aria-hidden='true' />
        <span className='netpulse__title'>Interfold network</span>
        <span className='netpulse__net'>{NETWORK_NAME}</span>
      </div>
      <div className='netpulse__grid'>
        <Tile value={network ? network.registeredNodes.toLocaleString() : '—'} label='ciphernodes registered' />
        <Tile value={network ? network.activeOperators.toLocaleString() : '—'} label='active now' />
        <Tile
          value={network ? fmtBonded(network.totalBonded, network.bondDecimals) : '—'}
          label={`${network?.bondSymbol ?? ''} bonded`.trim() || 'bonded'}
        />
        <Tile value={String(data.activeNow)} label={`active E3${data.activeNow === 1 ? '' : 's'} right now`} />
        <Tile value={data.ballots24h.toLocaleString()} label='encrypted ballots, last 24h' />
        <Tile value={data.pollsAllTime.toLocaleString()} label='CRISP polls, all-time' />
      </div>
    </section>
  )
}
