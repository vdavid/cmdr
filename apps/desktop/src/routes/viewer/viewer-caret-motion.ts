/**
 * The pure motion model behind the viewer's keyboard selection: given where the focus is
 * and which motion the user asked for, where does the focus go?
 *
 * Keyboard extension is "keep the anchor, move the focus", so this module answers only
 * the focus half. The five motions are a discriminated union, which makes the whole key
 * map an exhaustive switch the caller can't half-implement.
 *
 * Pure: no DOM, no state, no layout engine. The caller supplies the line cache
 * (`getLineText`) and the file's line count (`getTotalLines`).
 *
 * Three shapes come back:
 * - `focus` set, `targetLine` matching it: the motion landed.
 * - `focus` null with a real `targetLine`: the line the motion wants isn't in the cache.
 *   The caller consumes the key, leaves the selection alone, and **still** scrolls to
 *   `targetLine`, which is what fetches the line so the next press lands. Returning a
 *   bare `null` instead would make the press a permanent no-op, since nothing would ever
 *   fetch the line it wanted. ❌ Never guess an offset on an unfetched line: it crosses
 *   the IPC boundary into `viewer_read_range`.
 * - `focus` equal to `from`: the motion ran into the edge of the file and there was
 *   nowhere to go.
 */

import { EOF_LINE, type LineOffset } from './selection.svelte'
import { findWordEndAfter, findWordStartBefore } from './viewer-word'

/** `-1` is left / up, `+1` is right / down. */
export type MotionDirection = -1 | 1

/**
 * What the pressed chord asked for. `char` steps one grapheme, `word` one word boundary,
 * `line` one LOGICAL line (not one visual row: everything else in the viewer, from
 * `scrollByLines` to the height map to the search jump, counts logical lines, and a
 * second coordinate system here would be the only one), `lineEdge` the start or end of
 * the current line, `docEdge` the start or end of the file.
 */
export type CaretMotion =
  | { kind: 'char'; direction: MotionDirection }
  | { kind: 'word'; direction: MotionDirection }
  | { kind: 'line'; direction: MotionDirection }
  | { kind: 'lineEdge'; direction: MotionDirection }
  | { kind: 'docEdge'; direction: MotionDirection }

export interface MoveFocusArgs {
  /**
   * Where the focus is now.
   *
   * ❌ Never the `EOF_LINE` sentinel: `moveFocus` throws on one. ⌘A in ByteSeek-no-index
   * mode parks the focus there, and the sentinel names no line that can ever be cached,
   * so the keyboard layer resolves it to the last rendered line before calling in. The
   * refusal can't live here: this module knows nothing about what's on screen, so all it
   * could hand back is the sentinel as its own `targetLine`, and the caller would scroll
   * to *that* on every press with no way to shrink the selection again.
   */
  from: LineOffset
  motion: CaretMotion
  /** Reads the cached text of a line, or `undefined` when it hasn't been fetched. */
  getLineText: (line: number) => string | undefined
  /** The file's line count, or `null` in ByteSeek mode before the line index lands. */
  getTotalLines: () => number | null
  /** The column a run of vertical motions is aiming for, or `null` to start a new run. */
  desiredColumn: number | null
}

export interface MoveFocusResult {
  /** Where the focus goes, or `null` when the target line isn't cached. */
  focus: LineOffset | null
  /**
   * The line the caller scrolls to, set on every result including the `null` one.
   *
   * Gotcha: `docEdge` down with no line count yet reports `EOF_LINE`, which names no
   * scrollable row. Treat that value as "scroll to the end of the file"
   * (`scroll.scrollToEnd()`), never as an argument to line arithmetic.
   */
  targetLine: number
  /** The column to carry into the next vertical motion; `null` after a horizontal one. */
  desiredColumn: number | null
}

/** Everything a motion needs to know about the file. */
interface MotionContext {
  getLineText: (line: number) => string | undefined
  getTotalLines: () => number | null
}

/** A motion's answer before the desired column is decided. */
interface Landing {
  focus: LineOffset | null
  targetLine: number
}

const stay = (from: LineOffset): Landing => ({ focus: from, targetLine: from.line })
const at = (focus: LineOffset): Landing => ({ focus, targetLine: focus.line })
const unfetched = (targetLine: number): Landing => ({ focus: null, targetLine })

/** Every motion but `line` ends a run of vertical steps, so it drops the desired column. */
const dropColumn = (landing: Landing): MoveFocusResult => ({ ...landing, desiredColumn: null })

/**
 * Moves the selection's focus by one motion.
 *
 * Vertical motions keep a desired column so that walking down through a short line and
 * back returns to the original column; every other motion clears it. The column is a
 * logical UTF-16 offset, matching the logical-line rule above.
 */
export function moveFocus({ from, motion, getLineText, getTotalLines, desiredColumn }: MoveFocusArgs): MoveFocusResult {
  if (from.line === EOF_LINE) {
    throw new Error(
      'moveFocus was given the end-of-file sentinel line as its origin. Resolve it to a real rendered line first (see MoveFocusArgs.from).',
    )
  }
  const context: MotionContext = { getLineText, getTotalLines }

  switch (motion.kind) {
    case 'char':
      return dropColumn(moveByChar(context, from, motion.direction))
    case 'word':
      return dropColumn(moveByWord(context, from, motion.direction))
    case 'line': {
      const column = desiredColumn ?? from.offset
      return { ...moveByLine(context, from, motion.direction, column), desiredColumn: column }
    }
    case 'lineEdge':
      return dropColumn(moveToLineEdge(context, from, motion.direction))
    case 'docEdge':
      return dropColumn(moveToDocEdge(context, from, motion.direction))
  }
}

/**
 * Whether `line` can be a line of this file. A known count settles it; with no count yet
 * (ByteSeek before the index lands) nothing here can say no, and the line cache decides
 * instead by handing back `undefined`.
 */
function withinFile(context: MotionContext, line: number): boolean {
  if (line < 0) return false
  const total = context.getTotalLines()
  return total === null || line < total
}

/**
 * Moves onto the line one step in `direction` and lands where `landing` says. Bails to
 * `stay` at the edge of the file, and to `unfetched` when the neighbouring line isn't
 * cached, so no offset is ever invented for a line whose text we haven't seen.
 */
function crossLine(
  context: MotionContext,
  from: LineOffset,
  direction: MotionDirection,
  landing: (lineText: string) => number,
): Landing {
  const target = from.line + direction
  if (!withinFile(context, target)) return stay(from)
  const text = context.getLineText(target)
  if (text === undefined) return unfetched(target)
  return at({ line: target, offset: landing(text) })
}

function moveByChar(context: MotionContext, from: LineOffset, direction: MotionDirection): Landing {
  const text = context.getLineText(from.line)
  if (text === undefined) return unfetched(from.line)

  if (direction === 1) {
    if (from.offset < text.length) return at({ line: from.line, offset: nextGraphemeBoundary(text, from.offset) })
    return crossLine(context, from, 1, () => 0)
  }
  if (from.offset > 0) return at({ line: from.line, offset: previousGraphemeBoundary(text, from.offset) })
  return crossLine(context, from, -1, (lineText) => lineText.length)
}

function moveByWord(context: MotionContext, from: LineOffset, direction: MotionDirection): Landing {
  const text = context.getLineText(from.line)
  if (text === undefined) return unfetched(from.line)

  const within = direction === 1 ? wordStopAfter(text, from.offset) : wordStopBefore(text, from.offset)
  if (within !== null) return at({ line: from.line, offset: within })

  // Nothing left to reach on this line. An empty or wordless neighbour is still a stop,
  // so a blank line between paragraphs doesn't get skipped over.
  if (direction === 1) return crossLine(context, from, 1, (lineText) => wordStopAfter(lineText, 0) ?? 0)
  return crossLine(context, from, -1, (lineText) => wordStopBefore(lineText, lineText.length) ?? lineText.length)
}

/**
 * The next stop to the right inside one line: the end of the next word, or the line end
 * when only punctuation and whitespace are left. `null` once the offset is already there.
 */
function wordStopAfter(lineText: string, offset: number): number | null {
  const end = findWordEndAfter(lineText, offset)
  if (end !== null) return end
  return offset < lineText.length ? lineText.length : null
}

/** The mirror of `wordStopAfter`: the start of the previous word, else the line start. */
function wordStopBefore(lineText: string, offset: number): number | null {
  const start = findWordStartBefore(lineText, offset)
  if (start !== null) return start
  return offset > 0 ? 0 : null
}

function moveByLine(context: MotionContext, from: LineOffset, direction: MotionDirection, column: number): Landing {
  const target = from.line + direction
  if (!withinFile(context, target)) return stay(from)
  const text = context.getLineText(target)
  if (text === undefined) return unfetched(target)
  return at({ line: target, offset: Math.min(column, text.length) })
}

function moveToLineEdge(context: MotionContext, from: LineOffset, direction: MotionDirection): Landing {
  if (direction === -1) return at({ line: from.line, offset: 0 })
  const text = context.getLineText(from.line)
  if (text === undefined) return unfetched(from.line)
  return at({ line: from.line, offset: text.length })
}

/**
 * Extends to the start or end of the file. Going down has three answers, and the middle
 * one is why there is no second end-of-file representation:
 * - the last line is cached → its exact end, in one press;
 * - it isn't (the common case on a large file, since the cache only holds fetched
 *   windows) → no offset, but the caller scrolls there, and a second press lands it;
 * - there is no line count at all (ByteSeek before the index) → the `EOF_LINE` sentinel,
 *   which `toRangeEnds` maps to `RangeEnd::Eof` so the backend resolves the true end.
 *   ❌ Don't reach for the sentinel merely because the last line isn't cached: that turns
 *   it into a live, movable focus, which is exactly what `MoveFocusArgs.from` refuses.
 *
 * Consequence worth knowing: ⌘+Shift+Down from mid-file then copy shows the "unknown
 * size" confirm on a large file, because `isWholeFileSelection` bails on a start past
 * `(0, 0)` and the per-line estimator hits a line with no known byte length.
 */
function moveToDocEdge(context: MotionContext, from: LineOffset, direction: MotionDirection): Landing {
  if (direction === -1) return at({ line: 0, offset: 0 })

  const total = context.getTotalLines()
  if (total === null) return at({ line: EOF_LINE, offset: 0 })
  if (total <= 0) return stay(from)

  const lastLine = total - 1
  const text = context.getLineText(lastLine)
  if (text === undefined) return unfetched(lastLine)
  return at({ line: lastLine, offset: text.length })
}

/**
 * The offset just past the grapheme cluster starting at `offset`, so an emoji, a ZWJ
 * sequence, or a base letter with a combining mark is one step rather than two to five.
 *
 * Offsets stay UTF-16 code units throughout; grapheme stepping only decides how many of
 * them one press covers. `viewer-pointer.ts` keeps caret geometry on codepoint
 * boundaries, and grapheme boundaries are a strict refinement of those, so the geometry
 * invariant still holds.
 */
function nextGraphemeBoundary(lineText: string, offset: number): number {
  const cluster = graphemesOf(lineText).containing(offset)
  if (cluster === undefined) return Math.min(offset + 1, lineText.length)
  return cluster.index + cluster.segment.length
}

/** The mirror of `nextGraphemeBoundary`: the start of the cluster ending at `offset`. */
function previousGraphemeBoundary(lineText: string, offset: number): number {
  const cluster = graphemesOf(lineText).containing(offset - 1)
  if (cluster === undefined) return Math.max(offset - 1, 0)
  return cluster.index
}

/**
 * Grapheme segments of one line. `containing()` rather than materialising every segment,
 * so a 100k-character log line doesn't allocate an array per arrow press.
 */
function graphemesOf(lineText: string): Intl.Segments {
  return new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(lineText)
}
