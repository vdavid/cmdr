/**
 * Selection model for the file viewer.
 *
 * Tracks a single contiguous range by logical `(line, offset)` endpoints, not DOM nodes,
 * because the viewer virtualizes everything: only ~100 lines around the viewport ever
 * exist in the DOM, and the native Selection API loses its anchor or focus the moment
 * one of them scrolls out and its DOM node is recycled.
 *
 * `offset` is a UTF-16 code-unit index (matches JS `String.length`, and matches the
 * UTF-16 columns the search engine already emits, so the whole frontend speaks one
 * unit). The backend converts to UTF-8 bytes at the IPC boundary, with a surrogate
 * clamp for offsets that land between the high and low surrogate of an astral
 * codepoint.
 *
 * Range semantics are half-open `[start, end)`: start line is included from `start.offset`
 * to its end, intermediate lines are full, end line is included from offset 0 up to but
 * not including `end.offset`. ⌘A on an N-line file sets `focus = { line: N - 1, offset:
 * lastLineLength }` so the last character is included.
 */

export interface LineOffset {
  /** Zero-based line number. */
  line: number
  /** UTF-16 code-unit index inside the line text. Never negative. */
  offset: number
}

export interface Selection {
  /** Where the gesture started. */
  anchor: LineOffset
  /** Where the gesture currently is. May be before or after `anchor`. */
  focus: LineOffset
}

/** Result of `normaliseSelection`: anchor and focus in document order. */
export interface NormalisedSelection {
  start: LineOffset
  end: LineOffset
}

/**
 * Compares two `LineOffset`s lexicographically by (line, offset).
 * Returns negative if `a < b`, zero if equal, positive if `a > b`.
 */
export function compareLineOffset(a: LineOffset, b: LineOffset): number {
  if (a.line !== b.line) return a.line - b.line
  return a.offset - b.offset
}

/** Returns `true` if `a` and `b` point to the same `(line, offset)`. */
export function lineOffsetEquals(a: LineOffset, b: LineOffset): boolean {
  return a.line === b.line && a.offset === b.offset
}

/**
 * Returns the selection with endpoints in document order. Reversed drags
 * (anchor below focus) collapse to the same shape here.
 */
export function normaliseSelection(sel: Selection): NormalisedSelection {
  if (compareLineOffset(sel.anchor, sel.focus) <= 0) {
    return { start: sel.anchor, end: sel.focus }
  }
  return { start: sel.focus, end: sel.anchor }
}

/**
 * Returns `true` if the selection has any selected content. Caret-style
 * `anchor == focus` selections render no `.selected` spans.
 */
export function isEmpty(sel: Selection | null): boolean {
  if (sel === null) return true
  return lineOffsetEquals(sel.anchor, sel.focus)
}

/**
 * Returns `true` if `lineNumber` falls anywhere inside the selection (start
 * line, end line, or any intermediate line). Empty selections return `false`.
 */
export function isLineInRange(sel: Selection | null, lineNumber: number): boolean {
  if (isEmpty(sel)) return false
  const { start, end } = normaliseSelection(sel as Selection)
  return lineNumber >= start.line && lineNumber <= end.line
}

/**
 * Returns the `[selStart, selEnd)` UTF-16 offset bounds for `lineNumber` inside
 * the selection, given the line's `lineLength` (UTF-16 units). Returns `null`
 * if the line isn't selected, or if the bounds collapse to zero on this line
 * (caret on an empty line, exact start==end on this line).
 *
 * For the start line of the selection: `[start.offset, lineLength]`.
 * For the end line: `[0, end.offset]`.
 * For an intermediate line: `[0, lineLength]`.
 * For a single-line selection: `[start.offset, end.offset]`.
 */
export function getLineSegmentBounds(
  sel: Selection | null,
  lineNumber: number,
  lineLength: number,
): { selStart: number; selEnd: number } | null {
  if (isEmpty(sel)) return null
  const { start, end } = normaliseSelection(sel as Selection)
  if (lineNumber < start.line || lineNumber > end.line) return null

  let selStart: number
  let selEnd: number
  if (lineNumber === start.line && lineNumber === end.line) {
    selStart = Math.min(start.offset, lineLength)
    selEnd = Math.min(end.offset, lineLength)
  } else if (lineNumber === start.line) {
    selStart = Math.min(start.offset, lineLength)
    selEnd = lineLength
  } else if (lineNumber === end.line) {
    selStart = 0
    selEnd = Math.min(end.offset, lineLength)
  } else {
    selStart = 0
    selEnd = lineLength
  }

  if (selStart >= selEnd) return null
  return { selStart, selEnd }
}

/**
 * Returns a selection covering the whole file, given the total number of lines
 * and the length of the last line (UTF-16 units). Returns `null` for empty
 * files (0 lines) — ⌘A on an empty file is a no-op.
 *
 * For a file of `N` lines, the range runs from `{ line: 0, offset: 0 }` to
 * `{ line: N - 1, offset: lastLineLength }` inclusive of the last character.
 */
export function makeSelectAll(totalLines: number, lastLineLength: number): Selection | null {
  if (totalLines <= 0) return null
  return {
    anchor: { line: 0, offset: 0 },
    focus: { line: totalLines - 1, offset: lastLineLength },
  }
}

/**
 * Estimates the UTF-8 byte length of the selected range, given a per-line byte
 * length lookup and per-line UTF-16 length lookup. Used by the copy flow to
 * pick a size tier (silent / confirm / refuse) before paying for the backend
 * read.
 *
 * `getLineByteLength(n)` is the file's UTF-8 byte length for line `n` INCLUDING
 * its trailing newline. `getLineUtf16Length(n)` is the line's UTF-16 unit count
 * EXCLUDING the trailing newline.
 *
 * For the start and end lines (partial), we scale: `bytes * (selUtf16 /
 * lineUtf16)`. This is an estimate because UTF-16 unit count and UTF-8 byte
 * count don't line up for non-ASCII, but it's close enough for tier
 * classification (we need order-of-magnitude correctness, not exact bytes).
 *
 * Newline accounting: intermediate lines and the start line contribute their
 * full byte length (which includes the trailing newline). The end line is
 * partial up to `end.offset`, so we don't include its newline.
 *
 * Returns `null` if any required line length is missing (`getLineByteLength`
 * or `getLineUtf16Length` returns `null`); the caller can route to the
 * "selection size unknown" branch.
 */
export function estimateSelectionBytes(
  sel: Selection | null,
  getLineByteLength: (line: number) => number | null,
  getLineUtf16Length: (line: number) => number | null,
): number | null {
  if (isEmpty(sel)) return 0
  const { start, end } = normaliseSelection(sel as Selection)

  if (start.line === end.line) {
    const utf16 = getLineUtf16Length(start.line)
    const bytes = getLineByteLength(start.line)
    if (utf16 === null || bytes === null) return null
    const selUtf16 = Math.max(0, Math.min(end.offset, utf16) - Math.min(start.offset, utf16))
    if (utf16 === 0) return 0
    // Subtract the newline byte from the line's byte length: the trailing newline isn't
    // part of the text content. The byte length includes it for whole-line accounting.
    const textBytes = Math.max(0, bytes - 1)
    return Math.round(textBytes * (selUtf16 / utf16))
  }

  let total = 0

  const startUtf16 = getLineUtf16Length(start.line)
  const startBytes = getLineByteLength(start.line)
  if (startUtf16 === null || startBytes === null) return null
  if (startUtf16 === 0) {
    // Empty start line: just the trailing newline (1 byte).
    total += 1
  } else {
    const startSelUtf16 = Math.max(0, startUtf16 - Math.min(start.offset, startUtf16))
    const startTextBytes = Math.max(0, startBytes - 1)
    total += Math.round(startTextBytes * (startSelUtf16 / startUtf16)) + 1
  }

  for (let i = start.line + 1; i < end.line; i++) {
    const lineBytes = getLineByteLength(i)
    if (lineBytes === null) return null
    total += lineBytes
  }

  const endUtf16 = getLineUtf16Length(end.line)
  const endBytes = getLineByteLength(end.line)
  if (endUtf16 === null || endBytes === null) return null
  if (endUtf16 === 0 || end.offset === 0) {
    // End at offset 0: contribute nothing from the end line.
  } else {
    const endSelUtf16 = Math.min(end.offset, endUtf16)
    const endTextBytes = Math.max(0, endBytes - 1)
    total += Math.round(endTextBytes * (endSelUtf16 / endUtf16))
  }

  return total
}

/**
 * Reactive selection state for the viewer. Owns the `Selection | null` and exposes
 * setters that match the gesture vocabulary (`setAnchor`, `setFocus`, `selectAll`,
 * `clear`). The pure helpers above operate on the value `selection` returns; they
 * don't need the composable, which makes them trivially testable.
 */
export function createViewerSelection() {
  let selection = $state<Selection | null>(null)

  function setAnchor(point: LineOffset): void {
    selection = { anchor: point, focus: point }
  }

  function setFocus(point: LineOffset): void {
    if (selection === null) {
      selection = { anchor: point, focus: point }
      return
    }
    selection = { anchor: selection.anchor, focus: point }
  }

  function selectAll(totalLines: number, lastLineLength: number): void {
    selection = makeSelectAll(totalLines, lastLineLength)
  }

  function clear(): void {
    selection = null
  }

  return {
    get selection() {
      return selection
    },
    setAnchor,
    setFocus,
    selectAll,
    clear,
  }
}
