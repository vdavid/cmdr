/**
 * `ColumnList`'s pure layout math: column demands, the max-min-fair split of the shared
 * width, the grid template string, and the virtual window. The component owns the DOM reads
 * (container width, row padding, fonts, row height); everything here is a plain function of
 * numbers so it's unit-testable without a layout engine. DETAILS.md § ColumnList.
 */

/**
 * Per-column measurement safety pad, in CSS pixels. Pretext measures via canvas while the
 * row lays out via DOM, and on WKWebView the two can disagree by a fraction of a pixel on
 * the same font. Without the pad a name measured to exactly the track width truncates to
 * `na…me` for no visible reason. Same constant and rationale as
 * `file-explorer/views/measure-column-widths.ts`.
 */
export const MEASUREMENT_PAD = 2

/** Rows drawn above and below the ones in view, so a fast scroll doesn't flash empty rows. */
export const OVERSCAN_ROWS = 20

/**
 * Rows drawn before the list knows its row height or viewport (the first frame, or jsdom).
 * Big enough that every list short enough not to need a scrollbar renders whole.
 */
export const FALLBACK_ROWS = 100

/** A track that never depends on the data: `px`, or `ch` of the row's own font. */
export type FixedWidth = { kind: 'fixed'; px: number } | { kind: 'fixed'; ch: number }

/**
 * How a column claims width.
 * - `fixed`: a constant track.
 * - `fit`: shrink-wraps its widest cell (or header). Shows `fallback` until measured, and
 *   for good when nothing can be measured (a windowed source, or no canvas).
 * - `share`: splits what the other tracks leave with the other `share` columns, by max-min
 *   fairness on each one's demand (see `splitShareColumns`). Never below `minPx`.
 */
export type ColumnWidth = FixedWidth | { kind: 'fit'; fallback: FixedWidth } | { kind: 'share'; minPx: number }

/** What the template builder needs to know about a column. */
export interface LayoutColumn {
  width: ColumnWidth
  /** Whether the column has a demand (its cells can be measured). */
  measured?: boolean
}

/** A grid track: a pixel width, or a flexible `minmax(<min>px, 1fr)` share of what's left. */
export type Track = { kind: 'fixed'; px: number } | { kind: 'flex'; minPx: number }

export function trackToCss(track: Track): string {
  return track.kind === 'fixed' ? `${String(track.px)}px` : `minmax(${String(track.minPx)}px, 1fr)`
}

function fixedToCss(width: FixedWidth): string {
  return 'px' in width ? `${String(width.px)}px` : `${String(width.ch)}ch`
}

function fixedToPx(width: FixedWidth, chPx: number): number {
  return 'px' in width ? width.px : width.ch * chPx
}

/**
 * The width, in CSS pixels, a column needs to show every cell uncut: the widest of the
 * header and the cells, plus the measurement pad, never below `minPx`, rounded up.
 */
export function columnDemand({ header, cells, minPx }: { header: number; cells: number[]; minPx: number }): number {
  let widest = header
  for (const w of cells) if (w > widest) widest = w
  return Math.ceil(Math.max(minPx, widest + MEASUREMENT_PAD))
}

/**
 * Wraps a measurer so every decimal digit counts as the font's widest one. Numbers render
 * with `font-variant-numeric: tabular-nums`, which canvas can't model (the canvas `font`
 * shorthand has no slot for it): a slight over-estimate, never a clip.
 */
export function createTabularMeasure(measure: (text: string) => number): (text: string) => number {
  let digit = '0'
  let digitWidth = -1
  for (const d of '0123456789') {
    const w = measure(d)
    if (w > digitWidth) {
      digitWidth = w
      digit = d
    }
  }
  return (text) => measure(text.replace(/[0-9]/g, digit))
}

/**
 * Splits `available` between the `share` columns by max-min fairness (water-filling): each
 * round, every column whose demand fits in an even share of what's left takes exactly its
 * demand, and the rest share the remainder. The columns still wanting more become flexible
 * tracks, which CSS splits evenly. When every column fits, the LAST one turns flexible so the
 * spare width lands there (for Search, blank space before the right-aligned Size column).
 * A `null` demand (an unmeasured column) always wants more.
 */
export function splitShareColumns({
  available,
  demands,
  minPx,
}: {
  available: number
  demands: (number | null)[]
  minPx: number[]
}): Track[] {
  const flex = (i: number): Track => ({ kind: 'flex', minPx: minPx[i] })
  if (available <= 0) return demands.map((_, i) => flex(i))
  const tracks: (Track | null)[] = demands.map(() => null)
  let open = demands.map((_, i) => i)
  let remaining = available
  for (;;) {
    const share = remaining / open.length
    const fits = open.filter((i) => {
      const demand = demands[i]
      return demand !== null && demand <= share
    })
    if (fits.length === 0) break
    for (const i of fits) {
      const px = demands[i] ?? 0
      tracks[i] = { kind: 'fixed', px }
      remaining -= px
    }
    open = open.filter((i) => !fits.includes(i))
    if (open.length === 0) break
  }
  if (open.length === 0 && tracks.length > 0) tracks[tracks.length - 1] = flex(tracks.length - 1)
  return tracks.map((track, i) => track ?? flex(i))
}

/**
 * The one `grid-template-columns` string the header and every row share. `demands` holds
 * each column's demand (`null` where it has none), or is `null` while nothing is measured.
 * `rowChrome` is a row's horizontal padding plus its column gaps; `chPx` is one `ch` of the
 * row font, so `ch` tracks count against the shared width too.
 */
export function buildGridTemplate({
  columns,
  demands,
  containerWidth,
  rowChrome,
  chPx,
}: {
  columns: LayoutColumn[]
  demands: (number | null)[] | null
  containerWidth: number
  rowChrome: number
  chPx: number
}): string {
  if (!demands) {
    return columns
      .map(({ width }) => {
        if (width.kind === 'fixed') return fixedToCss(width)
        if (width.kind === 'fit') return fixedToCss(width.fallback)
        return trackToCss({ kind: 'flex', minPx: width.minPx })
      })
      .join(' ')
  }
  const css: string[] = []
  const shareIndices: number[] = []
  let available = containerWidth - rowChrome
  columns.forEach(({ width, measured }, i) => {
    const demand = measured ? demands[i] : null
    if (width.kind === 'fixed') {
      css[i] = fixedToCss(width)
      available -= fixedToPx(width, chPx)
    } else if (width.kind === 'fit') {
      css[i] = demand === null ? fixedToCss(width.fallback) : `${String(demand)}px`
      available -= demand ?? fixedToPx(width.fallback, chPx)
    } else {
      shareIndices.push(i)
    }
  })
  const split = splitShareColumns({
    available,
    demands: shareIndices.map((i) => (columns[i].measured ? demands[i] : null)),
    minPx: shareIndices.map((i) => {
      const width = columns[i].width
      return width.kind === 'share' ? width.minPx : 0
    }),
  })
  shareIndices.forEach((columnIndex, k) => (css[columnIndex] = trackToCss(split[k])))
  return css.join(' ')
}

/**
 * The half-open range of rows to draw. Before the row height or viewport is known it draws
 * the first `FALLBACK_ROWS`, so a short list renders whole on the first frame.
 */
export function visibleRange({
  count,
  rowHeight,
  scrollTop,
  viewportHeight,
  overscan,
}: {
  count: number
  rowHeight: number
  scrollTop: number
  viewportHeight: number
  overscan: number
}): { start: number; end: number } {
  if (rowHeight <= 0 || viewportHeight <= 0) return { start: 0, end: Math.min(count, FALLBACK_ROWS) }
  const start = Math.min(count, Math.max(0, Math.floor(scrollTop / rowHeight) - overscan))
  const end = Math.min(count, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan)
  return { start, end: Math.max(start, end) }
}

/** The `scrollTop` that brings row `index` fully into view, or `undefined` if it already is. */
export function revealScrollTop({
  index,
  rowHeight,
  scrollTop,
  viewportHeight,
}: {
  index: number
  rowHeight: number
  scrollTop: number
  viewportHeight: number
}): number | undefined {
  const top = index * rowHeight
  const bottom = top + rowHeight
  if (top < scrollTop) return top
  if (bottom > scrollTop + viewportHeight) return bottom - viewportHeight
  return undefined
}
