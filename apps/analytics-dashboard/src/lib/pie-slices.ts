/**
 * Client-safe: the shaping behind the Acquisition trends' breakdown pies (`DonutChart.svelte`): folding
 * small slices into "Other", each slice's share, its color, and the ring geometry.
 *
 * Colors follow the slice's key, never its rank. Four hues, validated with the dataviz skill's
 * `validate_palette.js "#d55181,#00a5c8,#c97a1a,#8f6fe0" --mode dark --surface #1a1917 --pairs all`
 * (2026-11-10): worst all-pairs CVD ΔE 8.2, normal-vision ΔE 16.7, all ≥ 3:1 on the surface. All-pairs
 * because a pie's neighbors change with the counts. No fifth hue clears the floors against these four, so
 * a pie names at most four colored slices; neutral keys ("(none)", "unknown") wear the unknown grey and
 * "Other" a lighter grey, and every slice is named in the legend beside the ring.
 */
import { COLOR_UNKNOWN } from './colors.js'

/** The categorical hues, in slot order: magenta, cyan, amber, violet. */
export const pieSlotColors = ['#d55181', '#00a5c8', '#c97a1a', '#8f6fe0'] as const

/** The "Other" slice: a lighter grey than "unknown", so the two read apart in one pie. */
export const COLOR_PIE_OTHER = '#9b9ba4'

/** The key of the folded "Other" slice. */
export const otherKey = 'other'

/** One entry going into a pie: `key` picks the color, `label` is what the reader sees. */
export interface PieInput {
  key: string
  label: string
  count: number
  /** The entries this one sums, when it merges several (a site's subdomains); the tooltip lists them. */
  members?: PieInput[]
}

/** One drawn slice: its share of the whole and its color, plus what folded into it for "Other". */
export interface PieSlice extends PieInput {
  share: number
  color: string
  folded?: PieInput[]
}

export interface FoldOptions {
  /** The most slices drawn on their own; the rest fold into "Other". Defaults to one per hue. */
  maxNamed?: number
  /** A slice under this share of the total folds into "Other" (default 3%, about 11° of ring). */
  minShare?: number
  /** Keys painted the unknown grey rather than a hue ("(none)", "unknown"); they don't use up a hue. */
  neutral?: string[]
}

/** FNV-1a over the key: a stable starting slot, so a key's color doesn't depend on its rank. */
function hashKey(key: string): number {
  let h = 0x811c9dc5
  for (let i = 0; i < key.length; i++) {
    h ^= key.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return h >>> 0
}

/**
 * Each key's hue: its hashed slot, or the next free one when another key in the same pie took it. Keys
 * claim slots in alphabetical order, so a rank change never repaints a slice; only a different set of
 * named keys can move a colliding one. Past `pieSlotColors.length` keys the hues repeat.
 */
export function pieColors(keys: string[]): Map<string, string> {
  const colors = new Map<string, string>()
  const taken = new Set<number>()
  const n = pieSlotColors.length
  for (const key of [...new Set(keys)].sort()) {
    let slot = hashKey(key) % n
    for (let probe = 0; probe < n && taken.has(slot); probe++) slot = (slot + 1) % n
    taken.add(slot)
    if (taken.size >= n) taken.clear()
    colors.set(key, pieSlotColors[slot])
  }
  return colors
}

/**
 * Rank the entries biggest first, fold the small and the overflow into one "Other" slice at the end, and
 * give every slice its share of the total. Zero counts drop out; a lone entry that would fold stays named,
 * since an "Other" of one hides a name for nothing.
 */
export function foldSlices(items: PieInput[], options: FoldOptions = {}): PieSlice[] {
  const { maxNamed = pieSlotColors.length, minShare = 0.03, neutral = [] } = options
  const ranked = items.filter((i) => i.count > 0).sort((a, b) => b.count - a.count || a.key.localeCompare(b.key))
  const total = ranked.reduce((sum, i) => sum + i.count, 0)
  if (total === 0) return []

  const named: PieInput[] = []
  const folded: PieInput[] = []
  let hues = 0
  for (const entry of ranked) {
    const isNeutral = neutral.includes(entry.key)
    if (entry.count / total >= minShare && (isNeutral || hues < maxNamed)) {
      named.push(entry)
      if (!isNeutral) hues++
    } else {
      folded.push(entry)
    }
  }
  if (folded.length === 1 && (neutral.includes(folded[0].key) || hues < maxNamed)) {
    named.push(folded.pop() as PieInput)
  }

  const colors = pieColors(named.filter((e) => !neutral.includes(e.key)).map((e) => e.key))
  const slices: PieSlice[] = named.map((e) => ({
    ...e,
    share: e.count / total,
    color: neutral.includes(e.key) ? COLOR_UNKNOWN : (colors.get(e.key) ?? COLOR_UNKNOWN),
  }))
  if (folded.length > 0) {
    const count = folded.reduce((sum, e) => sum + e.count, 0)
    slices.push({ key: otherKey, label: 'Other', count, share: count / total, color: COLOR_PIE_OTHER, folded })
  }
  return slices
}

/** A coordinate for an SVG path: rounded to 0.01 so the `d` string stays short and stable. */
function n(value: number): string {
  return String(Math.round(value * 100) / 100)
}

/** The point at `frac` of the way round, clockwise from 12 o'clock. */
function at(frac: number, r: number, cx: number, cy: number): [string, string] {
  const angle = frac * 2 * Math.PI - Math.PI / 2
  return [n(cx + r * Math.cos(angle)), n(cy + r * Math.sin(angle))]
}

/**
 * The SVG path of a ring segment from `start` to `end` (fractions of the whole, clockwise from 12 o'clock),
 * between radii `r` and `inner`. A full ring is two halves: one SVG arc can't start and end on one point.
 */
export function donutArc(start: number, end: number, r: number, inner: number, cx: number, cy: number): string {
  if (end - start >= 0.9999) {
    return `${donutArc(start, start + 0.5, r, inner, cx, cy)} ${donutArc(start + 0.5, start + 1, r, inner, cx, cy)}`
  }
  const large = end - start > 0.5 ? 1 : 0
  const [x1, y1] = at(start, r, cx, cy)
  const [x2, y2] = at(end, r, cx, cy)
  const [x3, y3] = at(end, inner, cx, cy)
  const [x4, y4] = at(start, inner, cx, cy)
  return `M ${x1} ${y1} A ${n(r)} ${n(r)} 0 ${String(large)} 1 ${x2} ${y2} L ${x3} ${y3} A ${n(inner)} ${n(inner)} 0 ${String(large)} 0 ${x4} ${y4} Z`
}
