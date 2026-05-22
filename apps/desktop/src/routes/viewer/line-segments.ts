/**
 * Shared line segmenter for the viewer.
 *
 * Given a line's text, the set of search matches inside it, and the selection
 * bounds on that line, returns an ordered list of non-overlapping spans
 * tagged with their visual state. The viewer template walks the list and
 * renders each span with the right combination of `<mark>` (search hit) and
 * `.selected` class.
 *
 * Decision/Why: extract this from `viewer-search.svelte.ts` so search doesn't
 * need to know about selection (the two systems compose at the render layer,
 * not at the data layer). Keeps the pure math testable without involving
 * Svelte's reactive layer.
 */

export interface SegmentMatchInput {
  /** UTF-16 column where the match starts. */
  column: number
  /** UTF-16 length of the match. */
  length: number
  /** Whether this is the currently focused match (renders as `mark.active`). */
  active: boolean
}

export interface SelectionBoundsInput {
  /** UTF-16 selection start on this line (inclusive). */
  selStart: number
  /** UTF-16 selection end on this line (exclusive). */
  selEnd: number
}

export interface LineSegment {
  text: string
  highlight: boolean
  active: boolean
  selected: boolean
}

/**
 * Splits `lineText` into segments at every search-match and selection-bound
 * boundary. Each output span is uniformly `highlight`/`selected`.
 *
 * Selection bounds are half-open `[selStart, selEnd)`; a position equal to
 * `selEnd` is NOT selected. Search matches use `[column, column + length)`
 * the same way.
 *
 * If both `matches` is empty and `selectionBounds` is `null`, returns a
 * single-element array with the whole line unmarked.
 */
export function segmentLine(
  lineText: string,
  matches: readonly SegmentMatchInput[],
  selectionBounds: SelectionBoundsInput | null,
): LineSegment[] {
  if (matches.length === 0 && selectionBounds === null) {
    return [{ text: lineText, highlight: false, active: false, selected: false }]
  }

  // Collect every boundary position (cut point) the segments need to start/end at.
  // Use a Set to dedupe; then sort.
  const cuts = new Set<number>([0, lineText.length])
  for (const m of matches) {
    cuts.add(Math.max(0, Math.min(m.column, lineText.length)))
    cuts.add(Math.max(0, Math.min(m.column + m.length, lineText.length)))
  }
  if (selectionBounds !== null) {
    cuts.add(Math.max(0, Math.min(selectionBounds.selStart, lineText.length)))
    cuts.add(Math.max(0, Math.min(selectionBounds.selEnd, lineText.length)))
  }
  const sortedCuts = [...cuts].sort((a, b) => a - b)

  const segments: LineSegment[] = []
  for (let i = 0; i < sortedCuts.length - 1; i++) {
    const start = sortedCuts[i]
    const end = sortedCuts[i + 1]
    if (start === end) continue

    const mid = start
    let highlight = false
    let active = false
    for (const m of matches) {
      if (m.column <= mid && mid < m.column + m.length) {
        highlight = true
        if (m.active) active = true
      }
    }

    let selected = false
    if (selectionBounds !== null && selectionBounds.selStart <= mid && mid < selectionBounds.selEnd) {
      selected = true
    }

    segments.push({ text: lineText.slice(start, end), highlight, active, selected })
  }

  return segments
}
