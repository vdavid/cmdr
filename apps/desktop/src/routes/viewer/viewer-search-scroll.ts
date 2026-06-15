/**
 * Pure "scroll a search match into view" math, axis-generic. Given the active
 * match's rendered edges and the content viewport's edges along one axis (all in
 * the same viewport-relative coordinate space, e.g. from `getBoundingClientRect`),
 * returns the new scroll offset that centres the match, or `null` when it's
 * already comfortably visible (so stepping through nearby matches doesn't jump).
 *
 * Working from the real rendered rect (not a `column * charWidth` estimate) makes
 * this exact for wide-CJK / astral glyphs and, crucially, for word-wrapped lines:
 * a wrapped line is a single tall element, so the match's true vertical row falls
 * straight out of its rect with no need to model where the wrap breaks landed.
 */
export function recenterOffset(params: {
  /** Match's leading edge along the axis (top or left), viewport-relative. */
  markStart: number
  /** Match's trailing edge along the axis (bottom or right), viewport-relative. */
  markEnd: number
  /** Viewport's leading edge along the axis, viewport-relative. */
  viewStart: number
  /** Viewport's trailing edge along the axis, viewport-relative. */
  viewEnd: number
  /** Current scroll offset on this axis (`scrollTop` / `scrollLeft`). */
  currentScroll: number
  /** When true, always return the centring target (never null) even if the match
   *  is already visible. Used when jumping to a match whose line was off-screen,
   *  so the view actively converges on it; the in-view skip is for the gentle
   *  case (match already on screen) where we don't want to nudge a visible match. */
  forceCenter?: boolean
}): number | null {
  const { markStart, markEnd, viewStart, viewEnd, currentScroll, forceCenter } = params
  const viewSize = viewEnd - viewStart
  if (viewSize <= 0) return null

  if (!forceCenter) {
    // Leave a small edge margin so an in-view match isn't flush against an edge
    // and tiny sub-pixel drift doesn't trigger a needless scroll.
    const edgeMargin = viewSize * 0.1
    if (markStart >= viewStart + edgeMargin && markEnd <= viewEnd - edgeMargin) return null
  }

  const markCenter = (markStart + markEnd) / 2
  const viewCenter = (viewStart + viewEnd) / 2
  return Math.max(0, currentScroll + (markCenter - viewCenter))
}
