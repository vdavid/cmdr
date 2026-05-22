/**
 * Pure helpers for the drag-past-edge autoscroll loop.
 *
 * When the pointer drifts within `EDGE_AUTOSCROLL_PX` of the viewport's top or bottom
 * during a drag, the viewer scrolls in that direction at a speed proportional to how
 * far past the threshold the pointer is. This gives the user a way to extend a
 * selection past the visible buffer without flicking the wheel.
 */

/** Distance from the viewport edge at which autoscroll kicks in. */
export const EDGE_AUTOSCROLL_PX = 30

/** Max scroll speed in px per frame (~30 lines/frame at 18 px/line = 540 px/frame). */
const MAX_PX_PER_FRAME = 540

/**
 * Returns the autoscroll px-per-frame for the given pointer position relative to a
 * viewport range `[top, bottom]`. Positive means "scroll down", negative means
 * "scroll up", 0 means "no autoscroll".
 *
 * Speed scales linearly with how far past the threshold the pointer is, capped at
 * `MAX_PX_PER_FRAME`. The threshold is `EDGE_AUTOSCROLL_PX` from each edge.
 *
 * Pure: no DOM, no time, no side effects. Easy to unit-test.
 */
export function computeAutoscrollPxPerFrame(pointerY: number, viewportTop: number, viewportBottom: number): number {
  const distanceFromTop = pointerY - viewportTop
  const distanceFromBottom = viewportBottom - pointerY

  if (distanceFromTop < EDGE_AUTOSCROLL_PX) {
    // Scroll up. The closer to (or past) the top, the faster.
    const past = EDGE_AUTOSCROLL_PX - distanceFromTop
    const ratio = Math.min(1, past / EDGE_AUTOSCROLL_PX)
    return -Math.round(ratio * MAX_PX_PER_FRAME)
  }

  if (distanceFromBottom < EDGE_AUTOSCROLL_PX) {
    const past = EDGE_AUTOSCROLL_PX - distanceFromBottom
    const ratio = Math.min(1, past / EDGE_AUTOSCROLL_PX)
    return Math.round(ratio * MAX_PX_PER_FRAME)
  }

  return 0
}
