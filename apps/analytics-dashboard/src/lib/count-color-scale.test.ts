import { describe, it, expect } from 'vitest'
import {
  COUNT_SCALE_PURPLE_AT,
  COUNT_ZERO_COLOR,
  countColor,
  countHue,
  countScaleStops,
  exceedsCountScale,
} from './count-color-scale.js'

/** sRGB hex → [r, g, b] in 0–255. */
function rgb(hex: string): [number, number, number] {
  const n = Number.parseInt(hex.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

/** WCAG relative luminance of an sRGB hex. */
function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map((c) => {
    const s = c / 255
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

const surface = '#1a1917'

describe('the count color scale', () => {
  it('is pinned at 1,000 for purple, never auto-scaled', () => {
    expect(COUNT_SCALE_PURPLE_AT).toBe(1000)
  })

  it('walks the hue from red at 1 to purple at the constant, evenly in log space', () => {
    // With 1,000 the six steps land on powers of √10.
    expect(countHue(1)).toBe('red')
    expect(countHue(Math.sqrt(10))).toBe('orange')
    expect(countHue(10)).toBe('yellow')
    expect(countHue(10 ** 1.5)).toBe('green')
    expect(countHue(100)).toBe('cyan')
    expect(countHue(10 ** 2.5)).toBe('blue')
    expect(countHue(1000)).toBe('purple')
  })

  it('clamps everything above the constant to purple', () => {
    expect(countColor(1_000_000)).toBe(countColor(COUNT_SCALE_PURPLE_AT))
    expect(countColor(1001)).toBe(countColor(COUNT_SCALE_PURPLE_AT))
  })

  it('gives zero its own hueless neutral, distinct from every count', () => {
    expect(countColor(0)).toBe(COUNT_ZERO_COLOR)
    const [r, g, b] = rgb(COUNT_ZERO_COLOR)
    expect(Math.max(r, g, b) - Math.min(r, g, b)).toBeLessThanOrEqual(6)
    expect(countColor(1)).not.toBe(COUNT_ZERO_COLOR)
  })

  it('changes color between neighboring steps', () => {
    const colors = [1, 3, 10, 32, 100, 316, 1000].map(countColor)
    expect(new Set(colors).size).toBe(colors.length)
  })

  it('reads on the dark surface: every count color clears 4.5:1, so it can carry text', () => {
    for (const v of [1, 2, 3, 5, 10, 20, 32, 50, 100, 200, 316, 500, 1000]) {
      expect(contrast(countColor(v), surface), `count ${String(v)}`).toBeGreaterThanOrEqual(4.5)
    }
  })

  it('returns valid sRGB hex', () => {
    for (const v of [0, 1, 7, 64, 999, 5000]) expect(countColor(v)).toMatch(/^#[0-9a-f]{6}$/)
  })

  it('treats a fraction between zero and one as the red end', () => {
    expect(countColor(0.4)).toBe(countColor(1))
  })
})

describe('countScaleStops', () => {
  it('lists the legend stops from zero to the constant', () => {
    const stops = countScaleStops()
    expect(stops[0]).toEqual({ value: 0, color: COUNT_ZERO_COLOR })
    expect(stops.map((s) => s.value)).toEqual([0, 1, 3, 10, 32, 100, 316, 1000])
    expect(stops.at(-1)?.color).toBe(countColor(COUNT_SCALE_PURPLE_AT))
  })
})

describe('exceedsCountScale', () => {
  it('fires only once a count passes the constant', () => {
    expect(exceedsCountScale([5, 1000, null])).toBe(false)
    expect(exceedsCountScale([5, 1001])).toBe(true)
    expect(exceedsCountScale([])).toBe(false)
  })
})
