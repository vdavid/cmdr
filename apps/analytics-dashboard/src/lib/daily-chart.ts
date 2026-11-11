/** Client-safe types and geometry for `DailyChart.svelte`, kept here so the math stays testable. */

import { formatNumber, formatPercent } from './format.js'

/** One stacked layer of a column chart. `null` days draw nothing and read as "–" in the tooltip. */
export interface ColumnSeries {
  key: string
  label: string
  color: string
  values: (number | null)[]
  /** Draw the segment as 45° stripes of `color` instead of a solid fill (the "unknown channel" texture). */
  hatch?: boolean
  /**
   * The `key` of the layer that absorbs this one's segment on a day it's too thin to read (under
   * `foldBelowPx` tall), like a country into "Other". Drawing only: the tooltip keeps it.
   */
  foldInto?: string
}

/** A segment shorter than this many pixels draws as part of its `foldInto` layer; it adapts as the axis grows. */
export const foldBelowPx = 4

/**
 * Column `i`'s drawn segments, bottom up: each layer's value that day, except that a value under `minValue`
 * moves into its `foldInto` layer (when that layer is in `layers`). Zero and unknown values drop out. Pure.
 */
export function foldedColumn(
  layers: ColumnSeries[],
  i: number,
  minValue: number,
): { series: ColumnSeries; value: number }[] {
  const keys = new Set(layers.map((s) => s.key))
  const moved = new Map<string, number>()
  const folds = (s: ColumnSeries, v: number) => s.foldInto !== undefined && keys.has(s.foldInto) && v < minValue
  for (const s of layers) {
    const v = s.values[i] ?? 0
    if (v > 0 && folds(s, v)) moved.set(s.foldInto as string, (moved.get(s.foldInto as string) ?? 0) + v)
  }
  return layers
    .map((s) => {
      const v = s.values[i] ?? 0
      return { series: s, value: (folds(s, v) ? 0 : v) + (moved.get(s.key) ?? 0) }
    })
    .filter((seg) => seg.value > 0)
}

/** A legend entry, for charts whose layers are too many to list one by one. */
export interface LegendItem {
  label: string
  color: string
  hatch?: boolean
}

/**
 * A rate chart's background bands: below `yellow` is red (bad), `yellow` up to `green` is yellow (fair),
 * and `green` and up is green (good).
 */
export interface RateBands {
  yellow: number
  green: number
}

export type BandStatus = 'bad' | 'fair' | 'good'

/** The bands between 0 and `yMax`, bottom first, each clipped to the axis; a band above the top is dropped. */
export function bandSegments(bands: RateBands, yMax: number): { status: BandStatus; from: number; to: number }[] {
  const edges: [BandStatus, number, number][] = [
    ['bad', 0, bands.yellow],
    ['fair', bands.yellow, bands.green],
    ['good', bands.green, Infinity],
  ]
  return edges.filter(([, from]) => from < yMax).map(([status, from, to]) => ({ status, from, to: Math.min(to, yMax) }))
}

/** One line of the hover tooltip: the value leads, the label follows, keyed by a mark in the series color. */
export interface TooltipRow {
  label: string
  value: string
  color?: string
  mark?: 'box' | 'line' | 'dot'
  /** A striped box, matching a hatched segment. */
  hatch?: boolean
  /** A group heading inside the tooltip: drawn smaller and muted, value optional. */
  heading?: boolean
  /**
   * A breakdown row (one country or channel of the day): drawn as aligned count and share columns beside a
   * magnitude bar. `value` still carries the same numbers as text, for the slider's `aria-valuetext`.
   */
  breakdown?: TooltipBreakdown
}

/** The parts of a breakdown row, as `breakdownRows` shapes them. */
export interface TooltipBreakdown {
  count: string
  /** The formatted share of the day's total, or `''` when the total is unknown. */
  share: string
  /** The bar's length, 0–1, relative to the group's largest row. */
  fill: number
  /** Colored pieces of the bar, left to right, each a fraction (0–1) of this row's own count. */
  parts: { fraction: number; color: string; hatch?: boolean }[]
  /** A muted aside after the label, like "2 direct, 1 Homebrew". */
  detail?: string
}

/** One input to `breakdownRows`: a layer's count on the day, plus how its row and bar look. */
export interface BreakdownEntry {
  label: string
  color: string
  count: number | null
  hatch?: boolean
  detail?: string
  /** The bar's colored pieces as raw counts (summing to `count`); by default one piece in `color`. */
  parts?: { value: number; color: string; hatch?: boolean }[]
  /** A catch-all bucket ("Other") that sorts after every named row, whatever its count. */
  last?: boolean
}

/**
 * A day's breakdown as tooltip rows: drops empty (zero or unknown) entries, sorts the rest by count, largest
 * first (ties and `sort: false` keep the input order; `last` entries go at the end), and scales every bar to
 * the largest row, so big and small read apart at a glance. Shares are of `total`. Pure.
 */
export function breakdownRows(
  entries: BreakdownEntry[],
  total: number | null,
  { sort = true }: { sort?: boolean } = {},
): TooltipRow[] {
  const shown = entries.map((entry, order) => ({ entry, order, count: entry.count ?? 0 })).filter((e) => e.count > 0)
  if (sort) {
    shown.sort(
      (a, b) => Number(a.entry.last ?? false) - Number(b.entry.last ?? false) || b.count - a.count || a.order - b.order,
    )
  }
  const max = Math.max(0, ...shown.map((e) => e.count))
  return shown.map(({ entry, count }) => {
    const share = total !== null && total > 0 ? formatPercent(count / total) : ''
    const parts = (entry.parts ?? [{ value: count, color: entry.color, hatch: entry.hatch }])
      .filter((p) => p.value > 0)
      .map((p) => ({ fraction: p.value / count, color: p.color, ...(p.hatch ? { hatch: true } : {}) }))
    const text = [formatNumber(count), share].filter(Boolean).join(' · ')
    return {
      label: entry.label,
      value: entry.detail ? `${text} (${entry.detail})` : text,
      color: entry.color,
      ...(entry.hatch ? { hatch: true } : {}),
      breakdown: {
        count: formatNumber(count),
        share,
        fill: count / max,
        parts,
        ...(entry.detail ? { detail: entry.detail } : {}),
      },
    }
  })
}

/** Roughly how wide a column label renders at the chart's 10px tabular font. */
export function labelWidth(text: string): number {
  return text.length * 6 + 2
}

/** A short axis day: "Nov 3". */
export function shortDay(date: string): string {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' })
}

/**
 * The surface gap (px) a stacked segment of `height` px gives up to separate it from the one below: 1px,
 * and only once the segment is at least 4px tall. Regression anchor: a flat 2px gap per segment ate whole
 * one-visitor slices on a small day (20 countries at ~2.5px each), so the one big grey "Other" slice on top
 * read as most of the bar while the tooltip said 29%.
 */
export function stackGap(height: number): number {
  return height >= 4 ? 1 : 0
}

/** An SVG path for a column with 4px-rounded top corners and a square base (the dataviz mark spec). */
export function columnPath(x: number, y: number, w: number, h: number, radius = 4): string {
  if (h <= 0 || w <= 0) return ''
  const r = Math.min(radius, w / 2, h)
  return (
    `M${String(x)},${String(y + h)}V${String(y + r)}` +
    `Q${String(x)},${String(y)} ${String(x + r)},${String(y)}` +
    `H${String(x + w - r)}Q${String(x + w)},${String(y)} ${String(x + w)},${String(y + r)}` +
    `V${String(y + h)}Z`
  )
}

/** Split a series into runs of consecutive known values, so a line breaks across unknown days. */
export function lineRuns(values: (number | null)[]): { index: number; value: number }[][] {
  const runs: { index: number; value: number }[][] = []
  let current: { index: number; value: number }[] = []
  values.forEach((value, index) => {
    if (value === null) {
      if (current.length > 0) runs.push(current)
      current = []
    } else {
      current.push({ index, value })
    }
  })
  if (current.length > 0) runs.push(current)
  return runs
}
