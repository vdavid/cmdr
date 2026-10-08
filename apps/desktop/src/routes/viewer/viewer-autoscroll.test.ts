import { describe, it, expect } from 'vitest'

import { AUTOSCROLL_PX_PER_SEC_PER_PX_PAST, computeAutoscrollPxPerSecond } from './viewer-autoscroll'

describe('computeAutoscrollPxPerSecond', () => {
  // Use a notional viewport of [100, 500] (height 400) for these tests.
  const top = 100
  const bottom = 500

  it('returns 0 anywhere inside the viewport, edges included', () => {
    expect(computeAutoscrollPxPerSecond(300, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerSecond(top + 1, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerSecond(bottom - 1, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerSecond(top, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerSecond(bottom, top, bottom)).toBe(0)
  })

  it('scrolls up past the top edge and down past the bottom edge', () => {
    expect(computeAutoscrollPxPerSecond(top - 10, top, bottom)).toBeLessThan(0)
    expect(computeAutoscrollPxPerSecond(bottom + 10, top, bottom)).toBeGreaterThan(0)
  })

  // Regression anchor for the "it's at the end of the document instantly" report: a pointer
  // a few px past the edge has to crawl, a couple of lines a second, not hundreds.
  it('crawls just past the edge, so a couple of extra lines are reachable', () => {
    const lineHeight = 18
    const linesPerSecond = computeAutoscrollPxPerSecond(bottom + 2, top, bottom) / lineHeight
    expect(linesPerSecond).toBeGreaterThan(0)
    expect(linesPerSecond).toBeLessThanOrEqual(3)
  })

  it('speeds up in proportion to the distance past the edge, like WebKit', () => {
    expect(computeAutoscrollPxPerSecond(bottom + 10, top, bottom)).toBe(10 * AUTOSCROLL_PX_PER_SEC_PER_PX_PAST)
    expect(computeAutoscrollPxPerSecond(bottom + 40, top, bottom)).toBe(40 * AUTOSCROLL_PX_PER_SEC_PER_PX_PAST)
    expect(computeAutoscrollPxPerSecond(top - 40, top, bottom)).toBe(-40 * AUTOSCROLL_PX_PER_SEC_PER_PX_PAST)
  })
})
