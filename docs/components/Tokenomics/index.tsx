// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import React, { useCallback, useRef, useState } from 'react'
import classes from './Tokenomics.module.css'

// ---------------------------------------------------------------------------
// Source data
// ---------------------------------------------------------------------------

export const TOTAL_SUPPLY = 1_200_000_000

type Group = 'community' | 'other'

type Slice = {
  key: string
  // Minted FOLD, exactly as recorded onchain.
  total: number
  // Months of linear unlock after UNLOCK_START. A value of 1 marks an
  // allocation with no lock policy: it is liquid in full from the TGE.
  vestMonths: number
  // Printed share, when the rounded share would leave the column short of
  // 100%. The donut geometry always uses the exact share.
  displayPct?: number
  color: string
  group: Group
}

// Single source of truth for the donut, the unlock chart and the unlock table,
// so the three can never drift apart. The CCA sale, the investor rounds
// (Legion included) and the team allocation (BlockScience included) are each
// carried as one category, matching how they are minted onchain.
// Amounts sum to TOTAL_SUPPLY; the 1e-6 residual is the allocation sheet's own
// display rounding, and is far below anything either chart can resolve.
// Rounding every share to two decimals would print a column summing to
// 100.01%, because three of them round up and none rounds down. The
// largest-remainder method settles which one gives up its round-up: Investors
// has the smallest remainder of the three (0.536), so it prints at its floor.
// Community = vivid brand greens; Other = dark brand neutrals.
// Colors alternate light↔dark across stacking order for maximum area-chart legibility.
const ALLOCATION: Slice[] = [
  // Community (55.50%)
  { key: 'Foundation Treasury', total: 468_668_644.211315, vestMonths: 48, color: '#3A7D44', group: 'community' }, // vivid forest
  { key: 'CCA Sale', total: 150_000_000, vestMonths: 1, color: '#687d71', group: 'community' }, // brand sage
  { key: 'Airdrop', total: 47_280_000, vestMonths: 24, color: '#82F5AD', group: 'community' }, // brand bright mint
  // Other (44.50%)
  { key: 'Gnosis Guild', total: 240_000_000, vestMonths: 48, color: '#252525', group: 'other' }, // brand dark charcoal
  { key: 'Investors', total: 176_584_349.104518, displayPct: 14.71, vestMonths: 1, color: '#3A4E42', group: 'other' }, // dark muted forest
  { key: 'Team and Advisors', total: 117_467_006.684168, vestMonths: 24, color: '#8FAE96', group: 'other' }, // muted sage
]

const pctOf = (total: number) => (total / TOTAL_SUPPLY) * 100

const communitySlices = ALLOCATION.filter((d) => d.group === 'community')
const otherSlices = ALLOCATION.filter((d) => d.group === 'other')
const COMMUNITY_PCT = communitySlices.reduce((s, d) => s + pctOf(d.total), 0) // 55.49572035
const OTHER_PCT = otherSlices.reduce((s, d) => s + pctOf(d.total), 0) // 44.50427965

// Unlocked allocations are transferable in full at the TGE, while linear
// unlocks release nothing at t = 0, so their combined share is exactly the
// circulating supply at the TGE.
const TGE_PCT = ALLOCATION.filter((d) => d.vestMonths <= 1).reduce((s, d) => s + pctOf(d.total), 0) // 27.21536243

const COLOR_BY_KEY: Record<string, string> = Object.fromEntries(ALLOCATION.map((s) => [s.key, s.color]))

const GROUP_COLOR: Record<Group, string> = {
  community: '#3A5E3C', // brand forest green
  other: '#1C3A22', // dark forest
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const round2 = (n: number) => Math.round(n * 100) / 100

// Percentages display to two decimal places, whole numbers included, so the
// column reads as one aligned set of figures. Exact values are kept in
// ALLOCATION so the donut geometry stays true; only the printed figures are
// rounded.
const fmtNum = (n: number) => n.toFixed(2)

const fmtPct = (n: number) => `${fmtNum(round2(n))}%`

// Slice shares honour displayPct when set, and otherwise round the exact share
// derived from the minted amount.
const fmtShare = (d: Slice) => `${fmtNum(d.displayPct ?? round2(pctOf(d.total)))}%`

function polar(cx: number, cy: number, r: number, angleDeg: number): [number, number] {
  const a = ((angleDeg - 90) * Math.PI) / 180
  return [cx + r * Math.cos(a), cy + r * Math.sin(a)]
}

function donutSlice(cx: number, cy: number, rOuter: number, rInner: number, start: number, end: number): string {
  const [x1, y1] = polar(cx, cy, rOuter, end)
  const [x2, y2] = polar(cx, cy, rOuter, start)
  const [x3, y3] = polar(cx, cy, rInner, start)
  const [x4, y4] = polar(cx, cy, rInner, end)
  const large = end - start > 180 ? 1 : 0
  return [
    `M ${x1} ${y1}`,
    `A ${rOuter} ${rOuter} 0 ${large} 0 ${x2} ${y2}`,
    `L ${x3} ${y3}`,
    `A ${rInner} ${rInner} 0 ${large} 1 ${x4} ${y4}`,
    'Z',
  ].join(' ')
}

// ---------------------------------------------------------------------------
// Key parameters
// ---------------------------------------------------------------------------

export function KeyParameters() {
  const cards = [
    { label: 'Total Supply', value: '1.2B' },
    { label: 'Circulating Supply at TGE', value: fmtPct(TGE_PCT) },
  ]
  return (
    <div className={classes.stats}>
      {cards.map((c) => (
        <div key={c.label} className={classes.statCard}>
          <p className={classes.statLabel}>{c.label}</p>
          <div className={classes.statValue}>{c.value}</div>
        </div>
      ))}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Allocation donut + legend
// ---------------------------------------------------------------------------

// Community and Other groups are separated by a small angular gap in the donut
// so the two colour families read as distinct visual clusters.
const GROUP_GAP_DEG = 6
const TOTAL_SLICE_DEG = 360 - 2 * GROUP_GAP_DEG // 348°
const COMMUNITY_SPAN_DEG = (COMMUNITY_PCT / 100) * TOTAL_SLICE_DEG // ≈ 179.73°
const OTHER_SPAN_DEG = (OTHER_PCT / 100) * TOTAL_SLICE_DEG // ≈ 168.27°
const OTHER_START_DEG = COMMUNITY_SPAN_DEG + GROUP_GAP_DEG // ≈ 185.73°

export function AllocationPie() {
  const size = 260
  const cx = size / 2 // 130
  const cy = size / 2 // 130
  const rOuter = 122
  const rInner = 74
  const POP = 7

  const [hover, setHover] = useState<string | null>(null)
  const [pos, setPos] = useState({ x: 0, y: 0 })
  const [fromTable, setFromTable] = useState(false)
  const wrapRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)

  type Arc = Slice & { start: number; end: number; dx: number; dy: number }

  const buildArcs = (): Arc[] => {
    let cur = 0
    const out: Arc[] = []

    for (const d of communitySlices) {
      const start = cur
      const span = (pctOf(d.total) / COMMUNITY_PCT) * COMMUNITY_SPAN_DEG
      cur += span
      const end = cur
      const mid = (((start + end) / 2 - 90) * Math.PI) / 180
      out.push({ ...d, start, end, dx: Math.cos(mid) * POP, dy: Math.sin(mid) * POP })
    }

    cur = OTHER_START_DEG

    for (const d of otherSlices) {
      const start = cur
      const span = (pctOf(d.total) / OTHER_PCT) * OTHER_SPAN_DEG
      cur += span
      const end = cur
      const mid = (((start + end) / 2 - 90) * Math.PI) / 180
      out.push({ ...d, start, end, dx: Math.cos(mid) * POP, dy: Math.sin(mid) * POP })
    }

    return out
  }

  const arcs = buildArcs()

  const onMove = (e: React.MouseEvent) => {
    // While hovering from the table the tooltip is pinned to the donut arc, so
    // ignore cursor movement to avoid yanking it back to the pointer.
    if (fromTable) return
    const r = wrapRef.current?.getBoundingClientRect()
    if (r) setPos({ x: e.clientX - r.left, y: e.clientY - r.top })
  }

  // Position of a slice's tooltip on the donut, in wrapper-local coordinates.
  // Reads refs, so it must only be called from event handlers (not render).
  const arcTooltipPos = (key: string) => {
    const svg = svgRef.current
    const wrap = wrapRef.current
    if (!svg || !wrap) return null
    const arc = arcs.find((a) => a.key === key)
    if (!arc) return null
    const midAngle = (arc.start + arc.end) / 2
    const midR = (rOuter + rInner) / 2
    const [svgX, svgY] = polar(cx, cy, midR, midAngle)
    const svgRect = svg.getBoundingClientRect()
    const wrapRect = wrap.getBoundingClientRect()
    return {
      x: svgRect.left - wrapRect.left + svgX * (svgRect.width / size),
      y: svgRect.top - wrapRect.top + svgY * (svgRect.height / size),
    }
  }

  const onTableEnter = (key: string) => {
    setHover(key)
    setFromTable(true)
    const p = arcTooltipPos(key)
    if (p) setPos(p)
  }

  const onTableLeave = () => {
    setHover(null)
    setFromTable(false)
  }

  return (
    <div className={classes.allocation} ref={wrapRef} onMouseMove={onMove}>
      <div className={classes.pieWrap}>
        <svg ref={svgRef} width={size} height={size} viewBox={`0 0 ${size} ${size}`} role='img' aria-label='Token distribution by category'>
          {/* Donut slices */}
          {arcs.map((a) => {
            const isHover = hover === a.key
            return (
              <path
                key={a.key}
                d={donutSlice(cx, cy, rOuter, rInner, a.start, a.end)}
                fill={a.color}
                stroke='#ffffff'
                strokeWidth={2.5}
                style={{
                  transition: 'transform 0.18s ease, opacity 0.18s ease',
                  transform: isHover ? `translate(${a.dx}px, ${a.dy}px)` : 'none',
                  opacity: hover && !isHover ? 0.78 : 1,
                  cursor: 'pointer',
                }}
                onMouseEnter={() => {
                  setHover(a.key)
                  setFromTable(false)
                }}
                onMouseLeave={() => {
                  setHover(null)
                  setFromTable(false)
                }}
              />
            )
          })}

          {/* Centre label */}
          <text x={cx} y={cy - 6} textAnchor='middle' fontSize='30' fontWeight='700' fill='#0f2233'>
            1.2B
          </text>
          <text x={cx} y={cy + 16} textAnchor='middle' fontSize='11' letterSpacing='0.08em' fill='#6b7280'>
            TOTAL SUPPLY
          </text>
        </svg>
      </div>

      <table className={classes.legend}>
        <thead>
          <tr>
            <th>Category</th>
            <th className={classes.center}>Distribution</th>
          </tr>
        </thead>
        <tbody>
          <tr className={classes.groupRow}>
            <td colSpan={2} className={classes.groupLabel} style={{ color: GROUP_COLOR.community }}>
              Community<span className={classes.groupPct}>({fmtPct(COMMUNITY_PCT)})</span>
            </td>
          </tr>
          {communitySlices.map((d) => (
            <tr
              key={d.key}
              onMouseEnter={() => onTableEnter(d.key)}
              onMouseLeave={onTableLeave}
              style={{ background: hover === d.key ? 'rgba(15,23,42,0.05)' : undefined }}
            >
              <td className={classes.catCell}>
                <span className={classes.swatch} style={{ background: d.color }} />
                {d.key}
              </td>
              <td className={classes.center}>{fmtShare(d)}</td>
            </tr>
          ))}

          <tr className={classes.groupRow}>
            <td colSpan={2} className={classes.groupLabel} style={{ color: GROUP_COLOR.other }}>
              Other<span className={classes.groupPct}>({fmtPct(OTHER_PCT)})</span>
            </td>
          </tr>
          {otherSlices.map((d) => (
            <tr
              key={d.key}
              onMouseEnter={() => onTableEnter(d.key)}
              onMouseLeave={onTableLeave}
              style={{ background: hover === d.key ? 'rgba(15,23,42,0.05)' : undefined }}
            >
              <td className={classes.catCell}>
                <span className={classes.swatch} style={{ background: d.color }} />
                {d.key}
              </td>
              <td className={classes.center}>{fmtShare(d)}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {hover && (
        <div className={classes.tooltip} style={{ left: pos.x, top: pos.y }}>
          {hover}
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Vesting schedule — cumulative stacked-area chart (circulating supply)
// ---------------------------------------------------------------------------

// Stacking order: bottom → top. Shorter unlock periods at the base so the
// chart reads as progressively longer commitments toward the top. Amounts and
// unlock lengths come straight from ALLOCATION, so the bands, the donut and
// the unlock table can never disagree.
const VESTING: Slice[] = ['Investors', 'CCA Sale', 'Airdrop', 'Team and Advisors', 'Gnosis Guild', 'Foundation Treasury'].map(
  (k) => ALLOCATION.find((d) => d.key === k)!,
)

const UNLOCK_START = 'Sep 1, 2026'

const scheduleOf = (d: Slice) => (d.vestMonths <= 1 ? 'Unlocked' : `${d.vestMonths} month linear unlock from ${UNLOCK_START}`)

const MONTHS_AXIS = 48
const X_TICKS = [0, 12, 24, 36, 48]
const X_LABELS = ['TGE', '12 mo', '24 mo', '36 mo', '48 mo']
const Y_MAX = 1_200_000_000
const Y_TICKS = [0, 300_000_000, 600_000_000, 900_000_000, 1_200_000_000]
const Y_LABELS = ['0', '300M', '600M', '900M', '1.2B']

// Linear unlocks start after the TGE, so they release nothing at t = 0 and the
// curve at TGE is only the unrestricted allocations. This is what puts
// circulating supply at TGE at 27.22% of total supply.
function cumulative(v: Slice, t: number): number {
  if (v.vestMonths <= 1) return v.total
  return (v.total * Math.min(t, v.vestMonths)) / v.vestMonths
}

export function VestingSchedule() {
  const W = 860
  const H = 380
  const padL = 56
  const padR = 24
  const padT = 24
  const padB = 48
  const plotW = W - padL - padR
  const plotH = H - padT - padB

  const x = useCallback((t: number) => padL + (t / MONTHS_AXIS) * plotW, [padL, plotW])
  const y = useCallback((v: number) => padT + plotH - (v / Y_MAX) * plotH, [padT, plotH])

  const months = Array.from({ length: MONTHS_AXIS + 1 }, (_, t) => t)
  const bands = VESTING.map((v, vi) => {
    // Lower boundary is the cumulative stack of all bands beneath this one.
    const lower = months.map((t) => VESTING.slice(0, vi).reduce((s, prev) => s + cumulative(prev, t), 0))
    const upper = months.map((t, i) => lower[i] + cumulative(v, t))
    const top = months.map((t, i) => `${x(t).toFixed(1)},${y(upper[i]).toFixed(1)}`)
    const bot = months.map((t, i) => `${x(t).toFixed(1)},${y(lower[i]).toFixed(1)}`).reverse()
    return { key: v.key, color: COLOR_BY_KEY[v.key], d: `M ${top.join(' L ')} L ${bot.join(' L ')} Z` }
  })

  const [hover, setHover] = useState<string | null>(null)
  const [tip, setTip] = useState(false)
  const [pos, setPos] = useState({ x: 0, y: 0 })
  const [fromTable, setFromTable] = useState(false)
  const wrapRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)

  const onMove = (e: React.MouseEvent) => {
    // While hovering from the table the tooltip is pinned to the band, so
    // ignore cursor movement.
    if (fromTable) return
    const r = wrapRef.current?.getBoundingClientRect()
    if (r) setPos({ x: e.clientX - r.left, y: e.clientY - r.top })
  }

  const onBandTableEnter = (key: string) => {
    setHover(key)
    setTip(true)
    setFromTable(true)
    const bc = getBandCenter(key)
    if (bc) setPos(bc)
  }

  const onBandTableLeave = () => {
    setHover(null)
    setTip(false)
    setFromTable(false)
  }

  // Compute the centre of a vesting band at t=36 months, in container coords.
  // Reads refs, so it must only be called from event handlers (not render).
  const getBandCenter = (key: string): { x: number; y: number } | null => {
    if (!svgRef.current || !wrapRef.current) return null
    const T = 36
    let runL = 0
    for (const v of VESTING) {
      const c = cumulative(v, T)
      if (v.key === key) {
        const svgX = x(T)
        const svgY = (y(runL) + y(runL + c)) / 2
        const svgRect = svgRef.current.getBoundingClientRect()
        const wrapRect = wrapRef.current.getBoundingClientRect()
        return {
          x: svgRect.left - wrapRect.left + svgX * (svgRect.width / W),
          y: svgRect.top - wrapRect.top + svgY * (svgRect.height / H),
        }
      }
      runL += c
    }
    return null
  }

  return (
    <div className={classes.vesting} ref={wrapRef} onMouseMove={onMove}>
      <div className={classes.svgScroll}>
        <svg
          ref={svgRef}
          width='100%'
          viewBox={`0 0 ${W} ${H}`}
          role='img'
          aria-label='Cumulative circulating supply by category over time'
          style={{ minWidth: 600 }}
        >
          {/* Horizontal gridlines + y labels */}
          {Y_TICKS.map((t, i) => (
            <g key={t}>
              <line x1={padL} y1={y(t)} x2={W - padR} y2={y(t)} stroke='#ece3d4' strokeWidth={1} />
              <text x={padL - 10} y={y(t) + 4} textAnchor='end' fontSize='11' fill='#9aa3b0'>
                {Y_LABELS[i]}
              </text>
            </g>
          ))}

          {/* Stacked area bands */}
          {bands.map((b) => {
            const isHover = hover === b.key
            return (
              <path
                key={b.key}
                d={b.d}
                fill={b.color}
                stroke='#ffffff'
                strokeWidth={isHover ? 1.5 : 0.75}
                style={{
                  transition: 'opacity 0.18s ease, filter 0.18s ease',
                  opacity: hover && !isHover ? 0.72 : 1,
                  filter: isHover ? 'brightness(1.08)' : 'none',
                  cursor: 'pointer',
                }}
                onMouseEnter={() => {
                  setHover(b.key)
                  setTip(true)
                  setFromTable(false)
                }}
                onMouseLeave={() => {
                  setHover(null)
                  setTip(false)
                  setFromTable(false)
                }}
              />
            )
          })}

          {/* X ticks + labels */}
          {X_TICKS.map((m, i) => (
            <g key={m}>
              <line x1={x(m)} y1={padT} x2={x(m)} y2={padT + plotH} stroke='#ece3d4' strokeWidth={m === 0 ? 0 : 1} />
              <text x={x(m)} y={padT + plotH + 20} textAnchor='middle' fontSize='11' fill='#6b7280'>
                {X_LABELS[i]}
              </text>
            </g>
          ))}
        </svg>
      </div>

      <div className={classes.vtableWrap}>
        <table className={classes.legend}>
          <thead>
            <tr>
              <th>Category</th>
              <th className={classes.num}>Unlock Schedule</th>
            </tr>
          </thead>
          <tbody>
            <tr className={classes.groupRow}>
              <td colSpan={2} className={classes.groupLabel} style={{ color: GROUP_COLOR.community }}>
                Community
              </td>
            </tr>
            {communitySlices.map((v) => (
              <tr
                key={v.key}
                onMouseEnter={() => onBandTableEnter(v.key)}
                onMouseLeave={onBandTableLeave}
                style={{ background: hover === v.key ? 'rgba(15,23,42,0.05)' : undefined }}
              >
                <td className={classes.catCell}>
                  <span className={classes.swatch} style={{ background: COLOR_BY_KEY[v.key] }} />
                  {v.key}
                </td>
                <td className={classes.num}>{scheduleOf(v)}</td>
              </tr>
            ))}

            <tr className={classes.groupRow}>
              <td colSpan={2} className={classes.groupLabel} style={{ color: GROUP_COLOR.other }}>
                Other
              </td>
            </tr>
            {otherSlices.map((v) => (
              <tr
                key={v.key}
                onMouseEnter={() => onBandTableEnter(v.key)}
                onMouseLeave={onBandTableLeave}
                style={{ background: hover === v.key ? 'rgba(15,23,42,0.05)' : undefined }}
              >
                <td className={classes.catCell}>
                  <span className={classes.swatch} style={{ background: COLOR_BY_KEY[v.key] }} />
                  {v.key}
                </td>
                <td className={classes.num}>{scheduleOf(v)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {tip && hover && (
        <div className={classes.tooltip} style={{ left: pos.x, top: pos.y }}>
          {hover}
        </div>
      )}
    </div>
  )
}
