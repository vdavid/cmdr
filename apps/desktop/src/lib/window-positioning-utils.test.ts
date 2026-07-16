import { describe, it, expect } from 'vitest'
import {
  cascadeFromMain,
  cascadeOffset,
  centerOnMain,
  clampToMonitor,
  growRectForRail,
  isFullyOnScreen,
  nearestMonitor,
  resolveChildPosition,
  shrinkRectForRail,
} from './window-positioning-utils'

const SINGLE_MONITOR = [{ x: 0, y: 0, width: 1920, height: 1080 }]
const DUAL_MONITORS = [
  { x: 0, y: 0, width: 1920, height: 1080 },
  { x: 1920, y: 0, width: 2560, height: 1440 },
]
const MAIN = { x: 100, y: 100, width: 1280, height: 800 }

describe('isFullyOnScreen', () => {
  it('returns true when the rect lies inside one monitor', () => {
    expect(isFullyOnScreen({ x: 100, y: 100, width: 800, height: 600 }, SINGLE_MONITOR)).toBe(true)
  })

  it('returns false when any corner pokes outside all monitors', () => {
    expect(isFullyOnScreen({ x: 1500, y: 100, width: 800, height: 600 }, SINGLE_MONITOR)).toBe(false)
  })

  it('returns false when the rect straddles two monitors', () => {
    // 1900..2700 crosses the seam at x=1920
    expect(isFullyOnScreen({ x: 1900, y: 100, width: 800, height: 600 }, DUAL_MONITORS)).toBe(false)
  })

  it('returns true when the rect fits within the secondary monitor', () => {
    expect(isFullyOnScreen({ x: 2000, y: 100, width: 800, height: 600 }, DUAL_MONITORS)).toBe(true)
  })

  it('returns false when no monitors exist', () => {
    expect(isFullyOnScreen({ x: 0, y: 0, width: 100, height: 100 }, [])).toBe(false)
  })
})

describe('nearestMonitor', () => {
  it('returns null with no monitors', () => {
    expect(nearestMonitor({ x: 0, y: 0, width: 0, height: 0 }, [])).toBeNull()
  })

  it('picks the monitor whose center is closest to the rect center', () => {
    const rect = { x: 2200, y: 600, width: 100, height: 100 }
    expect(nearestMonitor(rect, DUAL_MONITORS)).toEqual(DUAL_MONITORS[1])
  })
})

describe('clampToMonitor', () => {
  const monitor = { x: 0, y: 0, width: 1920, height: 1080 }

  it('leaves an already-fitting rect untouched', () => {
    const rect = { x: 100, y: 100, width: 800, height: 600 }
    expect(clampToMonitor(rect, monitor)).toEqual(rect)
  })

  it('pulls a rect back inside when it overflows right/bottom', () => {
    const rect = { x: 1800, y: 1000, width: 800, height: 600 }
    expect(clampToMonitor(rect, monitor)).toEqual({ x: 1120, y: 480, width: 800, height: 600 })
  })

  it('pulls a rect back inside when it overflows left/top', () => {
    const rect = { x: -200, y: -200, width: 400, height: 300 }
    expect(clampToMonitor(rect, monitor)).toEqual({ x: 0, y: 0, width: 400, height: 300 })
  })

  it('shrinks dimensions that exceed the monitor', () => {
    const rect = { x: 0, y: 0, width: 3000, height: 2000 }
    expect(clampToMonitor(rect, monitor)).toEqual({ x: 0, y: 0, width: 1920, height: 1080 })
  })
})

describe('centerOnMain', () => {
  it('centers the child rect on the main rect', () => {
    const rect = centerOnMain(MAIN, { width: 400, height: 300 })
    expect(rect.x).toBeCloseTo(540) // 100 + (1280-400)/2
    expect(rect.y).toBeCloseTo(350) // 100 + (800-300)/2
    expect(rect.width).toBe(400)
    expect(rect.height).toBe(300)
  })

  it('still works when the child is larger than main (returns negative-offset center)', () => {
    const rect = centerOnMain(MAIN, { width: 1500, height: 1000 })
    expect(rect.x).toBeCloseTo(-10) // 100 + (1280-1500)/2 = -10
    expect(rect.y).toBeCloseTo(-0) // 100 + (800-1000)/2 = 0
  })
})

describe('cascadeOffset', () => {
  it('starts at zero and steps by 24', () => {
    expect(cascadeOffset(0)).toBe(0)
    expect(cascadeOffset(1)).toBe(24)
    expect(cascadeOffset(2)).toBe(48)
  })

  it('wraps at 8 by default', () => {
    expect(cascadeOffset(8)).toBe(0)
    expect(cascadeOffset(9)).toBe(24)
  })
})

describe('cascadeFromMain', () => {
  it('positions at main top-left with the cascade offset', () => {
    expect(cascadeFromMain(MAIN, { width: 800, height: 600 }, 0)).toEqual({
      x: 100,
      y: 100,
      width: 800,
      height: 600,
    })
    expect(cascadeFromMain(MAIN, { width: 800, height: 600 }, 2)).toEqual({
      x: 148,
      y: 148,
      width: 800,
      height: 600,
    })
  })
})

describe('resolveChildPosition', () => {
  const SIZE = { width: 600, height: 400 }

  it('uses the saved rect when it is fully on-screen', () => {
    const saved = { x: 300, y: 300, width: 600, height: 400 }
    expect(resolveChildPosition({ size: SIZE, main: MAIN, monitors: SINGLE_MONITOR, saved })).toEqual(saved)
  })

  it('clamps a stale saved rect that no longer fits any monitor', () => {
    const saved = { x: 3000, y: 1500, width: 600, height: 400 }
    const result = resolveChildPosition({ size: SIZE, main: MAIN, monitors: SINGLE_MONITOR, saved })
    expect(isFullyOnScreen(result, SINGLE_MONITOR)).toBe(true)
  })

  it('centers on main when there is no saved rect', () => {
    const result = resolveChildPosition({ size: SIZE, main: MAIN, monitors: SINGLE_MONITOR, saved: null })
    // 100 + (1280-600)/2 = 440 ; 100 + (800-400)/2 = 300
    expect(result.x).toBeCloseTo(440)
    expect(result.y).toBeCloseTo(300)
  })

  it('clamps the centered fallback when main is itself off-screen', () => {
    // Main lives entirely on the secondary monitor that doesn't exist anymore
    const lonelyMain = { x: 3000, y: 1500, width: 1280, height: 800 }
    const result = resolveChildPosition({ size: SIZE, main: lonelyMain, monitors: SINGLE_MONITOR, saved: null })
    expect(isFullyOnScreen(result, SINGLE_MONITOR)).toBe(true)
  })

  it('treats a split-across-monitors rect as stale', () => {
    const split = { x: 1700, y: 100, width: 800, height: 600 }
    const result = resolveChildPosition({ size: SIZE, main: MAIN, monitors: DUAL_MONITORS, saved: split })
    expect(isFullyOnScreen(result, DUAL_MONITORS)).toBe(true)
  })
})

describe('growRectForRail', () => {
  const MONITOR = { x: 0, y: 0, width: 1920, height: 1080 }

  it('grows rightward by the rail width when there is room, leaving the left edge put', () => {
    const rect = { x: 100, y: 100, width: 1080, height: 720 }
    const { rect: grown, grewBy, shiftedLeftBy } = growRectForRail(rect, 340, MONITOR)
    expect(grown).toEqual({ x: 100, y: 100, width: 1420, height: 720 })
    expect(grewBy).toBe(340)
    expect(shiftedLeftBy).toBe(0)
  })

  it('slides the window left when growing would push its right edge off the monitor', () => {
    const rect = { x: 700, y: 100, width: 1080, height: 720 }
    const { rect: grown, grewBy, shiftedLeftBy } = growRectForRail(rect, 340, MONITOR)
    // 700 + 1420 = 2120 > 1920, so x moves to 1920 - 1420 = 500
    expect(grown).toEqual({ x: 500, y: 100, width: 1420, height: 720 })
    expect(grewBy).toBe(340)
    expect(shiftedLeftBy).toBe(200)
  })

  it('caps the width at the monitor width, anchoring to the left edge', () => {
    const rect = { x: 0, y: 0, width: 1700, height: 720 }
    const { rect: grown, grewBy, shiftedLeftBy } = growRectForRail(rect, 340, MONITOR)
    // 1700 + 340 = 2040, capped to 1920, so it only grows by 220
    expect(grown).toEqual({ x: 0, y: 0, width: 1920, height: 720 })
    expect(grewBy).toBe(220)
    expect(shiftedLeftBy).toBe(0)
  })
})

describe('shrinkRectForRail', () => {
  const MONITOR = { x: 0, y: 0, width: 1920, height: 1080 }

  it('reverses a plain grow: shrinks by the grown amount, leaving the left edge put', () => {
    const rect = { x: 100, y: 100, width: 1420, height: 720 }
    expect(shrinkRectForRail(rect, 340, 0, MONITOR, 950)).toEqual({ x: 100, y: 100, width: 1080, height: 720 })
  })

  it('reverses a slide: shrinks and moves back right by the slid amount', () => {
    const rect = { x: 500, y: 100, width: 1420, height: 720 }
    expect(shrinkRectForRail(rect, 340, 200, MONITOR, 950)).toEqual({ x: 700, y: 100, width: 1080, height: 720 })
  })

  it('never shrinks below the minimum window width', () => {
    const rect = { x: 0, y: 0, width: 1000, height: 720 }
    expect(shrinkRectForRail(rect, 340, 0, MONITOR, 950)).toEqual({ x: 0, y: 0, width: 950, height: 720 })
  })
})
