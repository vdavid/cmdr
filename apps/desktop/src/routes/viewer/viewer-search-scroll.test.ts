import { describe, it, expect } from 'vitest'
import { recenterOffset } from './viewer-search-scroll'

describe('recenterOffset', () => {
  // Viewport spanning 100..900 (size 800) in viewport-relative coords, currently
  // scrolled to 1000.
  const view = { viewStart: 100, viewEnd: 900, currentScroll: 1000 }

  it('returns null when the match is comfortably in view', () => {
    // Mark at 400..440 sits well within the central band → no scroll.
    expect(recenterOffset({ ...view, markStart: 400, markEnd: 440 })).toBeNull()
  })

  it('centers a match below / right of the viewport', () => {
    // Mark at 1500..1540 is past viewEnd. markCenter 1520, viewCenter 500 →
    // 1000 + (1520 - 500) = 2020.
    expect(recenterOffset({ ...view, markStart: 1500, markEnd: 1540 })).toBe(2020)
  })

  it('centers a match above / left of the viewport', () => {
    // Mark at -300..-260 (before viewStart). markCenter -280 → 1000 + (-280 - 500) = 220.
    expect(recenterOffset({ ...view, markStart: -300, markEnd: -260 })).toBe(220)
  })

  it('clamps the result to 0 (never negative)', () => {
    // A match far before the viewport while barely scrolled would center negative.
    expect(recenterOffset({ markStart: -300, markEnd: -260, viewStart: 0, viewEnd: 800, currentScroll: 50 })).toBe(0)
  })

  it('recenters a match within the 10% edge margin', () => {
    // edgeMargin = 80, so the comfortable band is 180..820. A match at 840..860
    // is past the right margin → recenter. markCenter 850 → 1000 + (850 - 500) = 1350.
    expect(recenterOffset({ ...view, markStart: 840, markEnd: 860 })).toBe(1350)
  })

  it('returns a centring target for an in-view match when forceCenter is set', () => {
    // Same in-view match as the first case, but forceCenter overrides the skip:
    // markCenter 420, viewCenter 500 → 1000 + (420 - 500) = 920.
    expect(recenterOffset({ ...view, markStart: 400, markEnd: 440, forceCenter: true })).toBe(920)
  })

  it('returns null when the viewport has no size', () => {
    expect(
      recenterOffset({ markStart: 1500, markEnd: 1540, viewStart: 100, viewEnd: 100, currentScroll: 0 }),
    ).toBeNull()
  })
})
