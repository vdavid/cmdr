/**
 * Geometric caret search: turns a point into a UTF-16 offset inside one line's text.
 *
 * Pure and layout-engine-free. The caller supplies a `measure` that returns the box of
 * the codepoint covering a given offset; everything here is arithmetic on those boxes,
 * so the search is unit-testable without a browser.
 */

/** A box in viewport coordinates. Structurally compatible with `DOMRect`. */
export interface CaretRect {
  left: number
  right: number
  top: number
  bottom: number
}

/** One codepoint's box, plus the UTF-16 span it occupies in the line text. */
export interface CharBox {
  /** UTF-16 offset where the codepoint starts (inclusive). */
  start: number
  /** UTF-16 offset where it ends (exclusive). `end - start` is 2 for astral codepoints. */
  end: number
  rect: CaretRect
}

/**
 * Measures the codepoint covering `offset`. Implementations snap `offset` onto a
 * codepoint boundary, so the returned `start` / `end` never split a surrogate pair.
 * Returns `null` when the box can't be measured (a detached node, an unlaid-out line).
 */
export type MeasureChar = (offset: number) => CharBox | null

export interface CaretGeometrySearch {
  /** Total length of the line text, in UTF-16 code units. */
  length: number
  measure: MeasureChar
  /** Viewport x of the point being resolved. */
  x: number
  /** Viewport y of the point being resolved. */
  y: number
}

/**
 * Resolves a point to a UTF-16 offset in `[0, length]` by binary-searching the line's
 * character boxes. Boxes run in reading order (row first, then x within the row), which
 * makes the "is this character before or after the point?" test monotone, so the search
 * costs ~log2(length) measurements even on a 100k-character line.
 *
 * Clamping is what makes the caret reachable everywhere: a point left of a row's text
 * (the line-number gutter, the row's left padding) lands on the row's first offset, a
 * point right of it on the row's last offset (the wrap point on a wrapped line, matching
 * what an editor does), a point below every row on the end of the line.
 */
export function findOffsetByGeometry({ length, measure, x, y }: CaretGeometrySearch): number {
  if (length <= 0) return 0

  let lo = 0
  let hi = length - 1
  // Smallest offset proven to start after the point; `length` until one is found.
  let firstAfter = length

  while (lo <= hi) {
    const mid = (lo + hi) >>> 1
    const box = measure(mid)
    // Unmeasurable character: keep what earlier probes proved rather than guessing.
    if (box === null) return lo

    const side = compareCharToPoint(box.rect, x, y)
    if (side === 0) {
      // The point is on this codepoint: snap to whichever of its edges is nearer.
      return x - box.rect.left < box.rect.right - x ? box.start : box.end
    }
    if (side < 0) {
      lo = box.end
    } else {
      firstAfter = box.start
      hi = box.start - 1
    }
  }

  return firstAfter
}

/**
 * Orders one character box against the point in reading order: `-1` when the character
 * comes before the point, `1` when it comes after, `0` when the point is on it.
 */
function compareCharToPoint(rect: CaretRect, x: number, y: number): -1 | 0 | 1 {
  if (y < rect.top) return 1
  if (y >= rect.bottom) return -1
  if (x >= rect.right) return -1
  if (x < rect.left) return 1
  return 0
}
