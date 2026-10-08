/**
 * Pure helpers for the drag-past-edge autoscroll loop.
 *
 * When the pointer leaves the viewport through its top or bottom during a drag, the
 * viewer scrolls that way at a speed proportional to how far past the edge the pointer
 * is. It's WebKit's own selection autoscroll, which Binary and Hex modes get natively
 * (their rows use native selection): no band inside the edge, a crawl just past it, and
 * faster the further the user pulls.
 */

/**
 * Scroll speed in px/s for each px the pointer sits past the edge.
 *
 * WebKit's autoscroll timer fires every 50 ms and scrolls just far enough to reveal the
 * pointer, so it moves the distance past the edge 20 times a second. Text mode draws its
 * own selection (`user-select: none`), so it has to reproduce that curve itself.
 */
export const AUTOSCROLL_PX_PER_SEC_PER_PX_PAST = 20

/**
 * Returns the autoscroll speed in px/s (content px, before any scroll scaling) for a
 * pointer at `pointerY` against a viewport spanning `[viewportTop, viewportBottom]`.
 * Positive scrolls down, negative scrolls up, 0 means no autoscroll.
 *
 * Gotcha/Why: ❌ don't add a band inside the edge, and don't cap it low. The previous
 * curve hit 540 px per FRAME 30 px inside the edge, so a drag that crossed the bottom
 * reached the end of the file before the user could stop at the next two lines.
 */
export function computeAutoscrollPxPerSecond(pointerY: number, viewportTop: number, viewportBottom: number): number {
  if (pointerY < viewportTop) return -(viewportTop - pointerY) * AUTOSCROLL_PX_PER_SEC_PER_PX_PAST
  if (pointerY > viewportBottom) return (pointerY - viewportBottom) * AUTOSCROLL_PX_PER_SEC_PER_PX_PAST
  return 0
}
