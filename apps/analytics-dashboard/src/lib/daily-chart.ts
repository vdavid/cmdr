/** Client-safe types and geometry for `DailyChart.svelte`, kept here so the math stays testable. */

/** One stacked layer of a column chart. `null` days draw nothing and read as "–" in the tooltip. */
export interface ColumnSeries {
  key: string
  label: string
  color: string
  values: (number | null)[]
  /** Draw the segment as 45° stripes of `color` instead of a solid fill (the "unknown channel" texture). */
  hatch?: boolean
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
}

/** Roughly how wide a column label renders at the chart's 10px tabular font. */
export function labelWidth(text: string): number {
  return text.length * 6 + 2
}

/** A short axis day: "Nov 3". */
export function shortDay(date: string): string {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' })
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
