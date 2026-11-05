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
 * what an editor does).
 *
 * Vertically, a point that no character box covers snaps onto the nearest visual row and
 * keeps its x. That's the leading: a text `Range`'s rect is the font's ascent + descent,
 * not the line box, so a band above and below every visual row belongs to no character.
 * ❗ Without the snap, a point in the band between rows N and N+1 resolves to the wrap
 * point whatever its x, and a drag whose ends both sit in it selects nothing (#962). The
 * same holds above the first row and below the last, so a point above every row lands on
 * the first row and one below every row on the last; the caller decides what "past the
 * rendered rows" means before it gets here.
 */
export function findOffsetByGeometry(search: CaretGeometrySearch): number {
  if (search.length <= 0) return 0
  const offset = searchByGeometry(search)
  const y = nearestRowY(search, offset)
  return y === null ? offset : searchByGeometry({ ...search, y })
}

/**
 * When `y` falls on no character box, the vertical middle of the visual row nearest it,
 * else `null`. `offset` is the first search's answer, so the rows around the point are
 * the ones holding the characters just before and at it.
 */
function nearestRowY({ length, measure, y }: CaretGeometrySearch, offset: number): number | null {
  const candidates: CaretRect[] = []
  for (const at of [offset - 1, offset]) {
    if (at < 0 || at >= length) continue
    const box = measure(at)
    if (box === null) return null
    if (y >= box.rect.top && y < box.rect.bottom) return null
    candidates.push(box.rect)
  }
  let nearest: CaretRect | null = null
  let nearestDistance = Number.POSITIVE_INFINITY
  for (const rect of candidates) {
    const distance = y < rect.top ? rect.top - y : y - rect.bottom
    if (distance < nearestDistance) {
      nearest = rect
      nearestDistance = distance
    }
  }
  return nearest === null ? null : (nearest.top + nearest.bottom) / 2
}

function searchByGeometry({ length, measure, x, y }: CaretGeometrySearch): number {
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
