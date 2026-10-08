/**
 * Selection model for the file viewer.
 *
 * Tracks a single contiguous range by logical `(row, offset)` endpoints, not DOM nodes,
 * because the viewer virtualizes everything: only ~100 rows around the viewport ever
 * exist in the DOM, and the native Selection API loses its anchor or focus the moment
 * one of them scrolls out and its DOM node is recycled.
 *
 * A ROW, not a physical line: a row ends at a newline or at a segment boundary
 * (`file_viewer::rows`), so a long line occupies several rows and every coordinate here
 * counts rows. The physical line number rides along on the row, for the gutter alone.
 *
 * `offset` is a UTF-16 code-unit index (matches JS `String.length`, and matches the
 * UTF-16 columns the search engine already emits, so the whole frontend speaks one
 * unit). The backend converts to UTF-8 bytes at the IPC boundary, with a surrogate
 * clamp for offsets that land between the high and low surrogate of an astral
 * codepoint.
 *
 * Range semantics are half-open `[start, end)`: the start row is included from
 * `start.offset` to its end, intermediate rows are full, the end row is included from
 * offset 0 up to but not including `end.offset`. ⌘A on an N-row file sets
 * `focus = { row: N - 1, offset: lastRowLength }` so the last character is included.
 */

import { tString } from '$lib/intl/messages.svelte'
import type { RangeEnd } from '$lib/tauri-commands'

export interface RowOffset {
  /** Zero-based ROW index. A row, never a physical line number. */
  row: number
  /** UTF-16 code-unit index inside the row text. Never negative. */
  offset: number
}

export interface Selection {
  /** Where the gesture started. */
  anchor: RowOffset
  /** Where the gesture currently is. May be before or after `anchor`. */
  focus: RowOffset
}

/**
 * Args for selecting the whole file: its total ROW count and the length of the
 * last row. Shared by `createViewerSelection().selectAll` and
 * `KeyboardDeps.selection.selectAll` (`viewer-keyboard.ts`): both took two
 * same-typed `number`s a caller could swap.
 */
export interface SelectAllArgs {
  totalRows: number
  lastRowLength: number
}

/** Result of `normaliseSelection`: anchor and focus in document order. */
export interface NormalisedSelection {
  start: RowOffset
  end: RowOffset
}

/**
 * Compares two `RowOffset`s lexicographically by (row, offset).
 * Returns negative if `a < b`, zero if equal, positive if `a > b`.
 */
export function compareRowOffset(a: RowOffset, b: RowOffset): number {
  if (a.row !== b.row) return a.row - b.row
  return a.offset - b.offset
}

/** Returns `true` if `a` and `b` point to the same `(row, offset)`. */
export function rowOffsetEquals(a: RowOffset, b: RowOffset): boolean {
  return a.row === b.row && a.offset === b.offset
}

/**
 * Returns the selection with endpoints in document order. Reversed drags
 * (anchor below focus) collapse to the same shape here.
 */
export function normaliseSelection(sel: Selection): NormalisedSelection {
  if (compareRowOffset(sel.anchor, sel.focus) <= 0) {
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
  return rowOffsetEquals(sel.anchor, sel.focus)
}

/**
 * Returns `true` if `rowNumber` falls anywhere inside the selection (start
 * row, end row, or any intermediate row). Empty selections return `false`.
 */
export function isRowInRange(sel: Selection | null, rowNumber: number): boolean {
  if (isEmpty(sel)) return false
  const { start, end } = normaliseSelection(sel as Selection)
  return rowNumber >= start.row && rowNumber <= end.row
}

/**
 * Returns the `[selStart, selEnd)` UTF-16 offset bounds for `rowNumber` inside
 * the selection, given the row's `rowLength` (UTF-16 units). Returns `null`
 * if the row isn't selected, or if the bounds collapse to zero on this row
 * (caret on an empty row, exact start==end on this row).
 *
 * For the start row of the selection: `[start.offset, rowLength]`.
 * For the end row: `[0, end.offset]`.
 * For an intermediate row: `[0, rowLength]`.
 * For a single-row selection: `[start.offset, end.offset]`.
 */
export function getRowSegmentBounds(
  sel: Selection | null,
  rowNumber: number,
  rowLength: number,
): { selStart: number; selEnd: number } | null {
  if (isEmpty(sel)) return null
  const { start, end } = normaliseSelection(sel as Selection)
  if (rowNumber < start.row || rowNumber > end.row) return null

  let selStart: number
  let selEnd: number
  if (rowNumber === start.row && rowNumber === end.row) {
    selStart = Math.min(start.offset, rowLength)
    selEnd = Math.min(end.offset, rowLength)
  } else if (rowNumber === start.row) {
    selStart = Math.min(start.offset, rowLength)
    selEnd = rowLength
  } else if (rowNumber === end.row) {
    selStart = 0
    selEnd = Math.min(end.offset, rowLength)
  } else {
    selStart = 0
    selEnd = rowLength
  }

  if (selStart >= selEnd) return null
  return { selStart, selEnd }
}

/**
 * Returns a selection covering the whole file, given the total number of rows
 * and the length of the last row (UTF-16 units). Returns `null` for empty
 * files (0 rows), so a fresh ⌘A on an empty file is a no-op.
 *
 * For a file of `N` rows, the range runs from `{ row: 0, offset: 0 }` to
 * `{ row: N - 1, offset: lastRowLength }` inclusive of the last character.
 *
 * `totalRows` is only ever a counted row total. Selecting to the end of a file whose
 * row count is not known yet is `makeSelectToEof()`, which mints `EOF_ROW` directly
 * instead of arriving at it by arithmetic.
 */
export function makeSelectAll(totalRows: number, lastRowLength: number): Selection | null {
  if (totalRows <= 0) return null
  return {
    anchor: { row: 0, offset: 0 },
    focus: { row: totalRows - 1, offset: lastRowLength },
  }
}

/**
 * Row index standing for "the end of the file" in a selection whose focus can't name a
 * real row: a ⌘A in ByteSeek-no-index mode before the index exists, or one whose last
 * row is not cached. `toRangeEnds` maps it to `RangeEnd::Eof` so the backend resolves
 * the true end itself, and the copy-size estimator reads it to take the known file size
 * instead of a per-row walk over rows that were never fetched.
 *
 * `Number.MAX_SAFE_INTEGER` so plain `compareRowOffset` ordering sorts it after every
 * real row, which keeps `normaliseSelection` and the per-row renderers working with no
 * special case.
 *
 * ❌ Never reconstruct this value by arithmetic. `makeSelectToEof` is the only
 * producer; a `totalRows - 1` elsewhere lands one row short of it and every
 * consumer's literal comparison silently stops matching.
 */
export const EOF_ROW = Number.MAX_SAFE_INTEGER

/**
 * Returns the selection ⌘A mints when the file's total row count is not known yet
 * (ByteSeek before its index lands): the whole file, from its first character to
 * `EOF_ROW`.
 */
export function makeSelectToEof(): Selection {
  return {
    anchor: { row: 0, offset: 0 },
    focus: { row: EOF_ROW, offset: 0 },
  }
}

/** What the file-size shortcut needs to know about the file it is sizing a slice of. */
export interface FileExtent {
  /** Counted rows, or `null` while only an estimate exists (ByteSeek with no index). */
  totalRows: number | null
  /** The file's size in bytes, as `viewer_open` reported it. */
  totalBytes: number
  /** Cached text of the file's LAST row, or `null` when that row isn't cached. */
  lastRowText: string | null
}

/**
 * The exact byte size of a selection that starts at the very top of the file, measured
 * from the file's own size rather than by walking rows, or `null` when this shortcut
 * can't answer.
 *
 * The walk is what it replaces: ⌘A on a large file selects rows the user never scrolled
 * through, so the row cache can't service a per-row sum, while `totalBytes` is exact and
 * free. A selection that stops PARTWAY into the last row is still handled here, by
 * subtracting the bytes it leaves behind. Handing such a selection the whole file's size
 * is what invariant I3 forbids: the same number decides the confirm dialog and the
 * refusal, so it has to be the number that will actually be copied.
 *
 * Returns `null` (caller falls back to `estimateSelectionBytes`, and from there to the
 * "size unknown" confirm) when the selection starts anywhere but `(0, 0)`, when it stops
 * before the last row, or when the last row isn't cached so the leftover can't be
 * measured. ❗ The fallback direction is deliberate: asking is safe, guessing high is
 * not.
 */
export function selectionBytesFromFileSize(sel: Selection | null, file: FileExtent): number | null {
  if (sel === null) return null
  const { start, end } = normaliseSelection(sel)
  if (start.row !== 0 || start.offset !== 0) return null
  if (end.row === EOF_ROW) return file.totalBytes
  if (file.totalRows === null || end.row < file.totalRows - 1) return null
  if (file.lastRowText === null) return null
  const leftover = file.lastRowText.slice(Math.min(end.offset, file.lastRowText.length))
  return file.totalBytes - new TextEncoder().encode(leftover).length
}

/**
 * Converts a selection into the `(anchor, focus)` `RangeEnd` pair `viewer_read_range`
 * and `viewer_write_range_to_file` accept. Endpoints come out in document order, so a
 * reversed drag reads the same range. Returns `null` for no selection.
 *
 * An end at `EOF_ROW` becomes `RangeEnd::Eof`, so the backend resolves the end of the
 * file itself instead of receiving a row index no file has. That holds whether or not a
 * row count has arrived since the selection was made: `EOF_ROW` means end-of-file either
 * way, and `selectionBytesFromFileSize` reads it the same way.
 */
export function toRangeEnds(sel: Selection | null): { anchor: RangeEnd; focus: RangeEnd } | null {
  if (sel === null) return null
  const { start, end } = normaliseSelection(sel)
  return {
    anchor: { kind: 'row', row: start.row, offset: start.offset },
    focus: end.row === EOF_ROW ? { kind: 'eof' } : { kind: 'row', row: end.row, offset: end.offset },
  }
}

/**
 * Maximum number of intermediate rows the AT (VoiceOver) announcement loop walks
 * before falling back to a generic "extends past visible content" message. Caps the
 * 9e15-row worst case from ⌘A in ByteSeek-no-index mode (where `focus.row` is
 * `EOF_ROW`).
 */
export const MAX_ANNOUNCE_ROWS = 10_000

/**
 * What the announcement needs to know about one row, as the backend stated it.
 *
 * ❗ `lineNumber` is the 0-based PHYSICAL line the row starts, or `null` on a
 * continuation row Cmdr began itself. It is a fact the backend states, ❌ never inferred
 * from the row index: on a wrapped file the two diverge, and the announcement is the one
 * place a listener has no gutter to check against.
 */
export interface AnnouncedRow {
  /** UTF-16 code units in the row's own text. */
  utf16Length: number
  /** 0-based physical line this row STARTS, or `null` when it continues the one above. */
  lineNumber: number | null
}

/**
 * The physical line a row belongs to, or `null` when it can't be known from the cache.
 *
 * Walks up from `row` to the nearest row that STARTS a line, because a continuation row
 * carries no number of its own. Two bounds keep it cheap: it stops at the first row the
 * cache doesn't hold (eviction keeps a window around the viewport, so that is near), and
 * it walks at most `MAX_ANNOUNCE_ROWS`. Row 0 answers without the cache at all: by the
 * row rule it always starts the file's first line.
 */
function lineOfRow(row: number, getRow: (row: number) => AnnouncedRow | null): number | null {
  for (let i = row; i >= 0 && row - i <= MAX_ANNOUNCE_ROWS; i--) {
    if (i === 0) return 0
    const cached = getRow(i)
    if (cached === null) return null
    if (cached.lineNumber !== null) return cached.lineNumber
  }
  return null
}

/**
 * UTF-16 code units the selection covers, summing row texts and adding NOTHING between
 * them. A row Cmdr broke at a segment boundary is not followed by a newline, so counting
 * one per row would over-report on every minified file. (Newlines the file does hold
 * aren't counted either; the number is a character count of the selected text, and it has
 * always read that way.) A row the cache doesn't hold contributes 0.
 */
function countSelectedChars(start: RowOffset, end: RowOffset, getRow: (row: number) => AnnouncedRow | null): number {
  if (start.row === end.row) return end.offset - start.offset
  let chars = (getRow(start.row)?.utf16Length ?? 0) - start.offset
  for (let i = start.row + 1; i < end.row; i++) {
    chars += getRow(i)?.utf16Length ?? 0
  }
  return chars + end.offset
}

/**
 * Builds the live-region announcement string for the current selection. Pure: takes
 * a selection and a per-row lookup, returns the string the screen reader will speak.
 * Empty string means "nothing to announce".
 *
 * ❗ It announces PHYSICAL LINE numbers, the same ones the gutter draws, ❌ never row
 * indexes. A wrapped line occupies several rows, so a selection sitting inside one long
 * line is one line however many rows it covers, and a row index would name a coordinate
 * the file doesn't have and the screen doesn't show. When the line can't be resolved from
 * the cache the announcement drops the location rather than guessing one.
 *
 * The character count comes from `countSelectedChars`, which adds nothing between rows.
 *
 * Caps the row span at `MAX_ANNOUNCE_ROWS`; past that, returns a generic message
 * so the announcement work stays bounded (the alternative would freeze the UI on
 * ⌘A in ByteSeek-no-index mode where the focus row is `EOF_ROW`).
 */
export function describeSelectionForAt(sel: Selection | null, getRow: (row: number) => AnnouncedRow | null): string {
  if (sel === null) return ''
  const { start, end } = normaliseSelection(sel)
  if (start.row === end.row && start.offset === end.offset) return ''

  const startLine = lineOfRow(start.row, getRow)

  const rowSpan = end.row - start.row
  if (rowSpan > MAX_ANNOUNCE_ROWS) {
    return startLine === null
      ? tString('viewer.selection.toEndOfFileNoLine')
      : tString('viewer.selection.toEndOfFile', { line: String(startLine + 1) })
  }

  const totalChars = countSelectedChars(start, end, getRow)
  const endLine = start.row === end.row ? startLine : lineOfRow(end.row, getRow)
  if (startLine === null || endLine === null) {
    return tString('viewer.selection.charsOnly', { chars: totalChars })
  }
  if (startLine === endLine) {
    return tString('viewer.selection.singleLine', {
      chars: totalChars,
      line: String(startLine + 1),
    })
  }
  return tString('viewer.selection.multiLine', {
    startLine: String(startLine + 1),
    endLine: String(endLine + 1),
    chars: totalChars,
  })
}

/**
 * Shift-click extension: returns a new selection that runs from the current selection's
 * anchor (or `point` if there's no current selection) to `point`. Caller-owned
 * `anchor` is preserved; only the focus changes. This is the gesture-correct shape:
 * the user clicked a new endpoint; the anchor (where the original gesture started)
 * stays put.
 *
 * Pure: no DOM, no state. The composable just sets `selection = extendSelection(...)`.
 */
export function extendSelection(current: Selection | null, point: RowOffset): Selection {
  if (current === null) {
    return { anchor: point, focus: point }
  }
  return { anchor: current.anchor, focus: point }
}

/**
 * What the byte estimator needs to know about one row, as the file stores it.
 *
 * ❗ `delimiterBytes` is the whole point of this shape: whether a row is followed by a
 * delimiter is a FACT about the file, not something the arithmetic may assume. It's 0 for
 * the file's last row (nothing follows it, so a file with no trailing newline stops being
 * counted as if it had one) and 0 for a CONTINUATION row, one Cmdr ended at a segment
 * boundary: the next row resumes mid-line with no byte in between. `textBytes` never
 * includes it. A CRLF file needs no special case: all three backends keep the `\r` inside
 * the row text, so the delimiter is the single `\n`.
 */
export interface RowMetrics {
  /** UTF-8 bytes of the row's own text, delimiter excluded. */
  textBytes: number
  /** UTF-16 code units of that same text, so a partial offset can be prorated. */
  utf16Length: number
  /** Bytes of the delimiter that follows this row in the file. 0 when none does. */
  delimiterBytes: number
}

/**
 * The metrics of one cached row, from what the backend said about it.
 *
 * ❗ The delimiter is decided HERE, in one place, from two facts and no assumption. A row
 * Cmdr ended at a segment boundary (`continues`) is followed by no byte at all: the next
 * row resumes the same line. The file's last row is followed by nothing either, so a file
 * with no trailing newline stops being counted as if it had one. Assuming a delimiter per
 * row over-counts a minified file by a byte every 20 000, and those bytes pick the 10 MB
 * confirm tier and the 100 MB refusal (invariant I3).
 */
export function rowMetrics({
  text,
  continues,
  isLastRow,
}: {
  text: string
  continues: boolean
  isLastRow: boolean
}): RowMetrics {
  return {
    textBytes: new TextEncoder().encode(text).length,
    utf16Length: text.length,
    // A CRLF file needs no case: all three backends keep the `\r` inside the row text,
    // so the delimiter is the single `\n`.
    delimiterBytes: continues || isLastRow ? 0 : 1,
  }
}

/** A row's own bytes between two UTF-16 offsets, prorated by the selected fraction. */
function textBytesBetween(row: RowMetrics, from: number, to: number): number {
  if (row.utf16Length === 0) return 0
  const selected = Math.max(0, Math.min(to, row.utf16Length) - Math.min(from, row.utf16Length))
  return Math.round(row.textBytes * (selected / row.utf16Length))
}

/**
 * Estimates the UTF-8 byte length of the selected range from a per-row metrics lookup.
 * The copy flow uses it to pick a size tier (silent / confirm / refuse) before paying for
 * the backend read.
 *
 * Whole rows contribute their text plus their delimiter. A partial start or end row
 * prorates its bytes by the selected UTF-16 fraction (`textBytes * selUtf16 / utf16`),
 * which is an estimate for non-ASCII: UTF-16 units and UTF-8 bytes don't line up. Tier
 * classification needs order-of-magnitude correctness, not exact bytes. The delimiter
 * terms, by contrast, are exact, because a delimiter is either there or it isn't.
 *
 * Range semantics are half-open, so the END row contributes text only: whatever delimits
 * it sits past `end.offset` and isn't selected.
 *
 * Returns `null` if any row the walk needs has no metrics (not cached); the caller can
 * route to the "selection size unknown" branch.
 */
export function estimateSelectionBytes(
  sel: Selection | null,
  getRowMetrics: (row: number) => RowMetrics | null,
): number | null {
  if (isEmpty(sel)) return 0
  const { start, end } = normaliseSelection(sel as Selection)

  const startRow = getRowMetrics(start.row)
  if (startRow === null) return null

  if (start.row === end.row) {
    return textBytesBetween(startRow, start.offset, end.offset)
  }

  // The selection runs past the start row, so whatever delimits that row is inside it.
  let total = textBytesBetween(startRow, start.offset, startRow.utf16Length) + startRow.delimiterBytes

  for (let i = start.row + 1; i < end.row; i++) {
    const row = getRowMetrics(i)
    if (row === null) return null
    total += row.textBytes + row.delimiterBytes
  }

  const endRow = getRowMetrics(end.row)
  if (endRow === null) return null
  return total + textBytesBetween(endRow, 0, end.offset)
}

/**
 * Reactive selection state for the viewer. Owns the `Selection | null` and exposes
 * setters that match the gesture vocabulary (`setAnchor`, `setFocus`, `setRange`,
 * `selectAll`, `selectToEof`, `clear`). The pure helpers above operate on the value
 * `selection` returns; they don't need the composable, which makes them trivially
 * testable.
 */
export function createViewerSelection() {
  let selection = $state<Selection | null>(null)

  function setAnchor(point: RowOffset): void {
    selection = { anchor: point, focus: point }
  }

  function setFocus(point: RowOffset): void {
    if (selection === null) {
      selection = { anchor: point, focus: point }
      return
    }
    selection = { anchor: selection.anchor, focus: point }
  }

  /**
   * Sets both endpoints at once. Word- and line-granularity gestures re-derive the whole
   * selection from the pressed range on every move, so they land here rather than on
   * `setAnchor` + `setFocus`; a character drag genuinely moves one endpoint and keeps
   * using `setFocus`.
   *
   * One object param on purpose: two bare `RowOffset`s are exactly the confusable
   * positional pair `cmdr/no-confusable-callback-params` exists for.
   */
  function setRange({ anchor, focus }: Selection): void {
    selection = { anchor, focus }
  }

  function selectAll({ totalRows, lastRowLength }: SelectAllArgs): void {
    selection = makeSelectAll(totalRows, lastRowLength)
  }

  function selectToEof(): void {
    selection = makeSelectToEof()
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
    setRange,
    selectAll,
    selectToEof,
    clear,
  }
}
