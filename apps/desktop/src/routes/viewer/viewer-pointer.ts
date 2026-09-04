/**
 * Pointer-to-caret resolution for the viewer.
 *
 * Resolves a viewport point to a `{ line, offset }` `LineOffset` in the viewer's logical
 * coordinates (UTF-16 code units inside the line text).
 *
 * ❌ Never reach for `document.caretPositionFromPoint` / `caretRangeFromPoint` here. Some
 * WebKit builds return a plausible-but-wrong offset for text under `user-select: none`
 * (which `.file-content` is), and nothing at runtime can tell a wrong answer from a right
 * one. This module hit-tests the rendered rows itself and hands the character boxes to
 * the pure search in `viewer-caret-geometry.ts`.
 */

import { findOffsetByGeometry, type CaretRect, type CharBox, type MeasureChar } from './viewer-caret-geometry'
import type { LineOffset } from './selection.svelte'

/**
 * Resolves a point inside `.file-content` to a caret. Returns `null` only when the point
 * is outside the content box (the toolbar, the search bar, the status bar) or nothing is
 * rendered; every point over the content resolves, including the line-number gutter, the
 * row padding, and the blank area below the last line.
 */
export function caretFromPoint(content: HTMLElement, x: number, y: number): LineOffset | null {
  const box = content.getBoundingClientRect()
  if (x < box.left || x > box.right || y < box.top || y > box.bottom) return null
  return resolveCaret(content, x, y)
}

/**
 * Like `caretFromPoint`, but pulls the point into the content box first. Drag moves and
 * autoscroll steps use this: the pointer is routinely outside the viewport there (that's
 * what triggers autoscroll), and the selection has to keep extending rather than freeze.
 *
 * Past the top or bottom edge the aim also snaps to that side horizontally, so the drag
 * sweeps whole visual rows as they scroll by, the way an editor does. Past a side edge
 * only x is pulled in, so the row under the pointer still decides the line.
 */
export function caretFromPointClamped(content: HTMLElement, x: number, y: number): LineOffset | null {
  const box = content.getBoundingClientRect()
  if (y < box.top) return resolveCaret(content, box.left, box.top)
  if (y > box.bottom) return resolveCaret(content, box.right, box.bottom)
  return resolveCaret(content, clamp(x, box.left, box.right), y)
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

function resolveCaret(content: HTMLElement, x: number, y: number): LineOffset | null {
  const rows = content.querySelectorAll<HTMLElement>('[data-line]')
  if (rows.length === 0) return null

  const hit = locateLine(rows, y)
  const line = parseLineNumber(hit.el)
  if (line === null) return null
  const lineText = hit.el.querySelector<HTMLElement>('.line-text')
  if (lineText === null) return null

  const { length, measure } = charBoxes(lineText)
  if (hit.edge === 'above') return { line, offset: 0 }
  if (hit.edge === 'below') return { line, offset: length }
  return { line, offset: findOffsetByGeometry({ length, measure, x, y }) }
}

/** Where `y` fell relative to the rendered rows: past one end, or on a row. */
type LineEdge = 'above' | 'below' | null

interface LineHit {
  el: HTMLElement
  edge: LineEdge
}

/**
 * Finds the rendered `[data-line]` row containing `y`. Rows sit in ascending order, so a
 * binary search costs ~log2(rendered rows) rect reads instead of one per row.
 */
function locateLine(rows: NodeListOf<HTMLElement>, y: number): LineHit {
  const last = rows.length - 1
  if (y < rows[0].getBoundingClientRect().top) return { el: rows[0], edge: 'above' }
  if (y > rows[last].getBoundingClientRect().bottom) return { el: rows[last], edge: 'below' }

  let lo = 0
  let hi = last
  while (lo <= hi) {
    const mid = (lo + hi) >>> 1
    const box = rows[mid].getBoundingClientRect()
    if (y < box.top) hi = mid - 1
    else if (y > box.bottom) lo = mid + 1
    else return { el: rows[mid], edge: null }
  }
  // Rendered rows are contiguous, so a gap shouldn't happen; take the row after it
  // rather than dropping the gesture.
  return { el: rows[Math.min(lo, last)], edge: null }
}

function parseLineNumber(row: HTMLElement): number | null {
  const raw = row.getAttribute('data-line')
  if (raw === null) return null
  const n = Number.parseInt(raw, 10)
  if (Number.isNaN(n) || n < 0) return null
  return n
}

/** One text node inside `.line-text`, with its UTF-16 start offset in the whole line. */
interface TextRun {
  node: Text
  start: number
}

/**
 * Indexes the text nodes inside `.line-text` (search highlighting and the selection
 * paint split a line into nested `<mark>` / `<span>` elements) and returns a `measure`
 * over them. Walking the nodes is proportional to the number of spans, never to the
 * line's length, so a 100k-character line costs the same as a short one.
 */
function charBoxes(lineText: HTMLElement): { length: number; measure: MeasureChar } {
  const runs: TextRun[] = []
  let length = 0
  const walker = lineText.ownerDocument.createTreeWalker(lineText, NodeFilter.SHOW_TEXT)
  let node = walker.nextNode()
  while (node !== null) {
    const text = node as Text
    const units = (text.nodeValue ?? '').length
    if (units > 0) {
      runs.push({ node: text, start: length })
      length += units
    }
    node = walker.nextNode()
  }
  return { length, measure: (offset) => measureChar(runs, length, offset) }
}

/**
 * Measures the codepoint covering `offset`. The offset is snapped onto a codepoint
 * boundary first, so an astral character is measured (and reported) as one two-unit box
 * and the caret can never land between its surrogates. The snap is per text node, so a
 * surrogate pair a `<mark>` boundary split across two nodes degrades to two one-unit
 * boxes; the backend clamps such a lone surrogate at the IPC boundary anyway.
 */
function measureChar(runs: TextRun[], length: number, offset: number): CharBox | null {
  if (length === 0) return null
  const at = clamp(offset, 0, length - 1)
  const run = runs[findRun(runs, at)]
  const value = run.node.nodeValue ?? ''

  let local = at - run.start
  if (local > 0 && isLowSurrogate(value.charCodeAt(local)) && isHighSurrogate(value.charCodeAt(local - 1))) {
    local -= 1
  }
  const units = isHighSurrogate(value.charCodeAt(local)) && isLowSurrogate(value.charCodeAt(local + 1)) ? 2 : 1

  const rect = rangeRect(run.node, local, Math.min(local + units, value.length))
  if (rect === null) return null
  return { start: run.start + local, end: run.start + local + units, rect }
}

/** Index of the run holding `offset` (the last run starting at or before it). */
function findRun(runs: TextRun[], offset: number): number {
  let lo = 0
  let hi = runs.length - 1
  while (lo < hi) {
    const mid = (lo + hi + 1) >>> 1
    if (runs[mid].start <= offset) lo = mid
    else hi = mid - 1
  }
  return lo
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff
}

/** The painted box of `[from, to)` inside one text node, in viewport coordinates. */
function rangeRect(node: Text, from: number, to: number): CaretRect | null {
  const range = node.ownerDocument.createRange()
  range.setStart(node, from)
  range.setEnd(node, to)

  // A character pushed onto the next visual row can also report a zero-width rect at the
  // end of the row it left; the rect with width is the glyph itself.
  const rects = range.getClientRects()
  for (let i = 0; i < rects.length; i++) {
    if (rects[i].width > 0) return rects[i]
  }
  if (rects.length > 0) return rects[0]

  const bounds = range.getBoundingClientRect()
  return bounds.width > 0 || bounds.height > 0 ? bounds : null
}
