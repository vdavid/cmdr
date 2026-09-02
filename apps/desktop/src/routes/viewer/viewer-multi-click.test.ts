import { describe, it, expect } from 'vitest'

import { advanceMultiClick, MULTI_CLICK_INTERVAL_MS, MULTI_CLICK_SLOP_PX } from './viewer-multi-click'

describe('advanceMultiClick', () => {
  it('starts the cycle at one', () => {
    expect(advanceMultiClick(null, { x: 10, y: 10, time: 0 }).count).toBe(1)
  })

  it('counts up while the presses stay close in time and place', () => {
    const first = advanceMultiClick(null, { x: 10, y: 10, time: 0 })
    const second = advanceMultiClick(first, { x: 10, y: 10, time: 120 })
    const third = advanceMultiClick(second, { x: 10, y: 10, time: 240 })

    expect([first.count, second.count, third.count]).toEqual([1, 2, 3])
  })

  it('restarts the cycle on the fourth press, the way an editor does', () => {
    let press = advanceMultiClick(null, { x: 10, y: 10, time: 0 })
    for (const time of [100, 200]) press = advanceMultiClick(press, { x: 10, y: 10, time })
    expect(press.count).toBe(3)

    expect(advanceMultiClick(press, { x: 10, y: 10, time: 300 }).count).toBe(1)
  })

  it('restarts when the presses are further apart than the double-click interval', () => {
    const first = advanceMultiClick(null, { x: 10, y: 10, time: 0 })

    expect(advanceMultiClick(first, { x: 10, y: 10, time: MULTI_CLICK_INTERVAL_MS }).count).toBe(2)
    expect(advanceMultiClick(first, { x: 10, y: 10, time: MULTI_CLICK_INTERVAL_MS + 1 }).count).toBe(1)
  })

  it('restarts when the pointer drifted further than the slop between presses', () => {
    const first = advanceMultiClick(null, { x: 10, y: 10, time: 0 })

    expect(advanceMultiClick(first, { x: 10 + MULTI_CLICK_SLOP_PX, y: 10, time: 50 }).count).toBe(2)
    expect(advanceMultiClick(first, { x: 10 + MULTI_CLICK_SLOP_PX + 1, y: 10, time: 50 }).count).toBe(1)
    expect(advanceMultiClick(first, { x: 10, y: 10 - MULTI_CLICK_SLOP_PX - 1, time: 50 }).count).toBe(1)
  })

  it('restarts when the clock runs backwards', () => {
    // A pointer event timestamp is monotonic in practice; a negative gap means the
    // previous press came from another timeline, so it can't be part of this gesture.
    const first = advanceMultiClick(null, { x: 10, y: 10, time: 500 })

    expect(advanceMultiClick(first, { x: 10, y: 10, time: 499 }).count).toBe(1)
  })

  it('carries the latest press forward, so a slow drift never accumulates into a reset', () => {
    // Each press is compared against the one before it, not against the first: three
    // presses creeping 3 px at a time are still a triple-click.
    const first = advanceMultiClick(null, { x: 10, y: 10, time: 0 })
    const second = advanceMultiClick(first, { x: 13, y: 10, time: 100 })
    const third = advanceMultiClick(second, { x: 16, y: 10, time: 200 })

    expect(third).toEqual({ count: 3, x: 16, y: 10, time: 200 })
  })
})
