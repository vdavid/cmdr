import { describe, it, expect } from 'vitest'

import { EDGE_AUTOSCROLL_PX, computeAutoscrollPxPerFrame } from './viewer-autoscroll'

describe('computeAutoscrollPxPerFrame', () => {
  // Use a notional viewport of [100, 500] (height 400) for these tests.
  const top = 100
  const bottom = 500

  it('returns 0 when the pointer is comfortably inside the viewport', () => {
    expect(computeAutoscrollPxPerFrame(300, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerFrame(top + EDGE_AUTOSCROLL_PX + 1, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerFrame(bottom - EDGE_AUTOSCROLL_PX - 1, top, bottom)).toBe(0)
  })

  it('returns 0 right at the threshold (closed interval on the safe side)', () => {
    // Exactly at the threshold: still safe. (distanceFromTop = EDGE_AUTOSCROLL_PX → not less than).
    expect(computeAutoscrollPxPerFrame(top + EDGE_AUTOSCROLL_PX, top, bottom)).toBe(0)
    expect(computeAutoscrollPxPerFrame(bottom - EDGE_AUTOSCROLL_PX, top, bottom)).toBe(0)
  })

  it('returns a small negative value near the top edge', () => {
    // 5 px inside the threshold: ratio = (30 - 25) / 30 = 1/6. So roughly -90.
    const v = computeAutoscrollPxPerFrame(top + 25, top, bottom)
    expect(v).toBeLessThan(0)
    expect(v).toBeGreaterThan(-100)
  })

  it('returns a small positive value near the bottom edge', () => {
    const v = computeAutoscrollPxPerFrame(bottom - 25, top, bottom)
    expect(v).toBeGreaterThan(0)
    expect(v).toBeLessThan(100)
  })

  it('returns the maximum speed when the pointer is exactly at the edge', () => {
    expect(computeAutoscrollPxPerFrame(top, top, bottom)).toBe(-540)
    expect(computeAutoscrollPxPerFrame(bottom, top, bottom)).toBe(540)
  })

  it('clamps to the maximum when the pointer is past the edge', () => {
    expect(computeAutoscrollPxPerFrame(top - 100, top, bottom)).toBe(-540)
    expect(computeAutoscrollPxPerFrame(bottom + 100, top, bottom)).toBe(540)
  })

  it('scales monotonically: closer to the edge → faster', () => {
    const a = computeAutoscrollPxPerFrame(bottom - 25, top, bottom)
    const b = computeAutoscrollPxPerFrame(bottom - 10, top, bottom)
    const c = computeAutoscrollPxPerFrame(bottom - 1, top, bottom)
    expect(a).toBeLessThan(b)
    expect(b).toBeLessThan(c)
  })
})
