/**
 * Shrink-wrap math for the results table's Name column.
 *
 * The column used to be a flat `minmax(80px, 22ch)` track, so a list of `test` files
 * reserved 22 characters of width and the Path column next to it mid-truncated to
 * crumbs. These helpers narrow the track to what the rows currently ON SCREEN actually
 * need, the same idea as `file-explorer/views/measure-column-widths.ts` (which does it
 * for Ext / Size / Modified against `FullList`'s virtualized slice).
 *
 * Both functions are pure so the algorithm is unit-testable with mocked widths; the
 * component owns the DOM reads (scroll offset, viewport height, row height, the font)
 * and never reads back a width it just wrote — see `QueryResults.svelte`.
 */

/**
 * Ceiling on the Name track, in `ch` (the advance of "0" at the row's font). Matches the
 * `22ch` the fixed track used to carry, so a wall of long names looks exactly as it did
 * and only the narrow case gets tighter.
 */
export const NAME_COL_MAX_CH = 22

/**
 * Floor on the Name track, in CSS pixels. Matches the old `minmax(80px, …)` minimum: a
 * name longer than the ceiling still mid-truncates with an ellipsis, and a list of very
 * short names shouldn't squeeze the column down to a couple of glyphs.
 */
export const NAME_COL_MIN_PX = 80

/**
 * Per-cell measurement safety pad, in CSS pixels. Pretext measures via canvas while the
 * row lays out via DOM, and on WKWebView the two can disagree by a fraction of a pixel on
 * the same font. Without the pad a name measured to exactly the track width truncates to
 * `na…me` for no visible reason. Same constant and rationale as
 * `views/measure-column-widths.ts`.
 */
export const NAME_MEASUREMENT_PAD = 2

export interface NameColumnWidthArgs {
  /** The names rendered by the rows currently on screen, in row order. */
  names: string[]
  /** The column header's own label; the track must fit it or the header ellipsizes. */
  headerLabel: string
  /**
   * Pixel-accurate text measurer built at the ROW's font (bolder than the header's, so
   * measuring the header label with it slightly over-reserves — the safe direction).
   */
  measure: (text: string) => number
}

/**
 * Narrowest Name-column width, in CSS pixels, that fits every passed-in name plus the
 * header label, clamped to [`NAME_COL_MIN_PX`, `NAME_COL_MAX_CH` ch].
 */
export function computeNameColumnWidth({ names, headerLabel, measure }: NameColumnWidthArgs): number {
  // `ch` is the advance of "0", so the cap tracks the font the same way the old CSS
  // `22ch` did — including a text-size change, which rebuilds the measurer.
  const cap = measure('0') * NAME_COL_MAX_CH
  let widest = measure(headerLabel)
  for (const name of names) {
    const w = measure(name)
    if (w > widest) widest = w
  }
  return Math.ceil(Math.min(cap, Math.max(NAME_COL_MIN_PX, widest + NAME_MEASUREMENT_PAD)))
}

export interface RowRange {
  /** First visible row index, inclusive. */
  start: number
  /** Last visible row index, exclusive. */
  end: number
}

/**
 * Which rows a uniform-height, non-virtualized list currently shows.
 *
 * `QueryResults` renders every result (Search caps at 30 rows, Selection lists one
 * folder), so there's no virtualized window to borrow. Every input is independent of the
 * column width we're about to set, which is what keeps the measure → render → measure
 * loop impossible: rows are `white-space: nowrap` one-liners, so their height and the
 * container's scroll geometry can't move when the Name track resizes.
 *
 * Degenerate inputs (a height we couldn't measure, a container not laid out yet) fall
 * back to the whole list rather than to an empty range: over-measuring gives a slightly
 * wide column, under-measuring clips names.
 */
export function visibleRowRange(scrollTop: number, viewportHeight: number, rowHeight: number, count: number): RowRange {
  if (count <= 0) return { start: 0, end: 0 }
  if (rowHeight <= 0 || viewportHeight <= 0) return { start: 0, end: count }
  const start = Math.max(0, Math.min(count - 1, Math.floor(scrollTop / rowHeight)))
  const end = Math.min(count, Math.max(start + 1, Math.ceil((scrollTop + viewportHeight) / rowHeight)))
  return { start, end }
}
