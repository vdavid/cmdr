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

/**
 * The tallest scroll content the list lays out, in CSS pixels. WebKit stores layout lengths
 * as 1/64 px in an int32, so a box taller than 2^25 px (33,554,432) is silently clamped: at
 * the default 23.6 px row, every row past about 1,421,000 sat beyond the scrollbar's reach
 * (verified on macOS 27.0.1 WKWebView, a Swift probe padding a scroller to 47.2M px read
 * `scrollHeight` 33,554,432 and a max `scrollTop` 33,554,032, 2026-11-07). A list taller than
 * this scrolls through `scrollMapping`'s scaled position instead. Kept under the cap with
 * room for the drawn rows and rounding.
 */
export const MAX_SCROLL_HEIGHT_PX = 32_000_000

/** How a list's scroll position maps onto its rows. */
export interface ScrollMapping {
  /** The scroll content's height in CSS pixels: every row's, or the cap. */
  contentHeight: number
  /** Virtual pixels per scrolled pixel: 1 while the rows fit under the cap, more past it. */
  ratio: number
}

/**
 * The scroll content's height and its scale. A list that fits lays out every row's pixel
 * (ratio 1, today's plain spacers). Past `MAX_SCROLL_HEIGHT_PX`, the content is the cap and
 * one scrolled pixel moves `ratio` virtual ones, mapped so the scroller's last position shows
 * the last row: (virtual height − viewport) / (cap − viewport).
 */
export function scrollMapping({
  count,
  rowHeight,
  viewportHeight,
}: {
  count: number
  rowHeight: number
  viewportHeight: number
}): ScrollMapping {
  const virtualHeight = count * rowHeight
  if (virtualHeight <= MAX_SCROLL_HEIGHT_PX || viewportHeight >= MAX_SCROLL_HEIGHT_PX) {
    return { contentHeight: virtualHeight, ratio: 1 }
  }
  const viewport = Math.max(0, viewportHeight)
  return {
    contentHeight: MAX_SCROLL_HEIGHT_PX,
    ratio: (virtualHeight - viewport) / (MAX_SCROLL_HEIGHT_PX - viewport),
  }
}

/**
 * Where the drawn rows sit inside the scroll content: spacer padding above and below them,
 * plus a negative top margin when a scaled list's first rows belong above the content's top.
 *
 * With ratio 1 a row sits at `index × rowHeight`, so the spacers are the rows outside the
 * window. Scaled, row `index` must show at `index × rowHeight − virtualTop` below the
 * viewport's top, so the window starts at `start × rowHeight − (virtualTop − scrollTop)` in
 * the content: that can be negative near the top (row 0 already scrolled partly out), which
 * a padding can't say and a margin can. The bottom spacer keeps the content at
 * `contentHeight` either way.
 *
 * Scaled values are snapped to WebKit's 1/64 px layout unit: a difference of floats can land a
 * hair off zero, and `String()` writes 1.4e-8 in exponent form, which CSS rejects, so the
 * style kept its OLD spacer and the content doubled in height.
 */
export function windowPlacement({
  count,
  range,
  rowHeight,
  scrollTop,
  mapping,
}: {
  count: number
  range: { start: number; end: number }
  rowHeight: number
  scrollTop: number
  mapping: ScrollMapping
}): { padTop: number; padBottom: number; marginTop: number } {
  if (rowHeight <= 0) return { padTop: 0, padBottom: 0, marginTop: 0 }
  if (mapping.ratio === 1) {
    return { padTop: range.start * rowHeight, padBottom: (count - range.end) * rowHeight, marginTop: 0 }
  }
  const drawnHeight = (range.end - range.start) * rowHeight
  const virtualTop = scrollTop * mapping.ratio
  const top = toLayoutUnit(range.start * rowHeight - (virtualTop - scrollTop))
  return {
    padTop: Math.max(0, top),
    padBottom: Math.max(0, toLayoutUnit(mapping.contentHeight - top - drawnHeight)),
    marginTop: Math.min(0, top),
  }
}

/** Snaps a length to WebKit's layout unit, 1/64 px, which also keeps it out of exponent notation. */
function toLayoutUnit(px: number): number {
  return Math.round(px * 64) / 64
}

/**
 * The `scrollTop` that brings row `index` fully into view, or `undefined` if it already is.
 * `ratio` is a scaled list's `ScrollMapping.ratio`: the row is placed in virtual pixels and
 * the answer converted back to a scroll position.
 */
export function revealScrollTop({
  index,
  rowHeight,
  scrollTop,
  viewportHeight,
  ratio = 1,
}: {
  index: number
  rowHeight: number
  scrollTop: number
  viewportHeight: number
  ratio?: number
}): number | undefined {
  const virtualTop = scrollTop * ratio
  const top = index * rowHeight
  const bottom = top + rowHeight
  if (top < virtualTop) return top / ratio
  if (bottom > virtualTop + viewportHeight) return (bottom - viewportHeight) / ratio
  return undefined
}
