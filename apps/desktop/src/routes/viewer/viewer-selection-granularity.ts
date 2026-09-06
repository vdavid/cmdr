/**
 * Selection granularity for the viewer: how far a gesture snaps its endpoints out.
 *
 * A double-press selects a word and a triple-press a line, and dragging or shift-clicking
 * afterwards keeps working in those units, the way native text views do. Both endpoints
 * of the resulting `Selection` are edges of granularity ranges, so the drag re-derives
 * the whole selection from the pressed range on every move instead of nudging one end.
 *
 * That re-derivation is what makes a hand twitch harmless: a pointer that hasn't left the
 * pressed word yields the same union the press did, so there is nothing to collapse.
 *
 * Pure: no DOM, no state, no layout engine. The caller resolves the pointer to a
 * `LineOffset` (`viewer-pointer.ts`) and hands the line text in.
 */

import { findWordBoundsAt } from './viewer-word'
import type { LineOffset, Selection } from './selection.svelte'

/** How far a gesture snaps its endpoints out: to the caret, the word, or the whole line. */
export type SelectionGranularity = 'character' | 'word' | 'line'

/** A half-open `[start, end)` UTF-16 span inside one logical line. */
export interface LineRange {
  /** Zero-based line the span sits on. */
  line: number
  /** UTF-16 start offset, included. */
  start: number
  /** UTF-16 end offset, excluded. */
  end: number
}

interface RangeAtCaretArgs {
  /** The resolved pointer position. */
  caret: LineOffset
  granularity: SelectionGranularity
  /** Reads the cached text of a line, or `undefined` when it hasn't been fetched. */
  getLineText: (line: number) => string | undefined
}

interface ExtendArgs extends Omit<RangeAtCaretArgs, 'caret'> {
  /** The range the gesture started from, snapped at the same granularity. */
  anchorRange: LineRange
  /** Where the pointer is now. */
  focus: LineOffset
}

/**
 * Returns the range `caret` belongs to at `granularity`: itself for `character`, the
 * surrounding word for `word`, the whole logical line for `line` (offset 0 to its UTF-16
 * length, so a word-wrapped line still covers in full).
 *
 * A line the cache hasn't fetched yet reads as empty and collapses to `[0, 0)`. That
 * happens during a fast autoscroll into unfetched rows, where the row renders empty
 * anyway, so the selection matches what the user sees.
 */
export function rangeAtCaret({ caret, granularity, getLineText }: RangeAtCaretArgs): LineRange {
  if (granularity === 'character') return { line: caret.line, start: caret.offset, end: caret.offset }

  const lineText = getLineText(caret.line) ?? ''
  if (granularity === 'line') return { line: caret.line, start: 0, end: lineText.length }

  const { start, end } = findWordBoundsAt(lineText, caret.offset)
  return { line: caret.line, start, end }
}

/**
 * Returns the selection spanning `anchorRange` and the range `focus` falls in, with the
 * gesture's direction preserved: dragging forward anchors at the start of the pressed
 * range, dragging back past it anchors at the end. `normaliseSelection` orders either one
 * for rendering and for the IPC boundary, so the direction only has to stay honest for
 * the next move to read it.
 */
export function extendRangeToGranularity({ anchorRange, focus, granularity, getLineText }: ExtendArgs): Selection {
  const focusRange = rangeAtCaret({ caret: focus, granularity, getLineText })
  const backwards =
    focusRange.line < anchorRange.line || (focusRange.line === anchorRange.line && focusRange.start < anchorRange.start)

  if (backwards) {
    return {
      anchor: { line: anchorRange.line, offset: anchorRange.end },
      focus: { line: focusRange.line, offset: focusRange.start },
    }
  }
  return {
    anchor: { line: anchorRange.line, offset: anchorRange.start },
    focus: { line: focusRange.line, offset: focusRange.end },
  }
}
