import { describe, it, expect } from 'vitest'
import { COLOR_UNKNOWN } from './colors.js'
import { COLOR_PIE_OTHER, donutArc, foldSlices, pieColors, pieSlotColors } from './pie-slices.js'

const item = (key: string, count: number) => ({ key, label: key, count })

describe('foldSlices', () => {
  it('ranks slices biggest first and gives each its share of the total', () => {
    const slices = foldSlices([item('hn', 25), item('reddit', 75)])
    expect(slices.map((s) => [s.key, s.count, s.share])).toEqual([
      ['reddit', 75, 0.75],
      ['hn', 25, 0.25],
    ])
  })

  it('folds slices under the minimum share into one "Other" slice at the end, keeping the total', () => {
    const slices = foldSlices([item('a', 90), item('b', 4), item('c', 3), item('d', 3)], { minShare: 0.05 })
    expect(slices.map((s) => [s.key, s.count])).toEqual([
      ['a', 90],
      ['other', 10],
    ])
    expect(slices[1].label).toBe('Other')
    expect(slices[1].share).toBeCloseTo(0.1)
    expect(slices[1].folded?.map((f) => f.key)).toEqual(['b', 'c', 'd'])
  })

  it('folds everything past the named-slice cap, however big', () => {
    const slices = foldSlices([item('a', 30), item('b', 25), item('c', 20), item('d', 15), item('e', 10)], {
      maxNamed: 3,
      minShare: 0,
    })
    expect(slices.map((s) => [s.key, s.count])).toEqual([
      ['a', 30],
      ['b', 25],
      ['c', 20],
      ['other', 25],
    ])
  })

  it('keeps a lone small slice named rather than making an "Other" of one', () => {
    const slices = foldSlices([item('a', 98), item('b', 2)], { minShare: 0.05 })
    expect(slices.map((s) => s.key)).toEqual(['a', 'b'])
  })

  it('drops zero counts and returns nothing for an empty or all-zero input', () => {
    expect(foldSlices([])).toEqual([])
    expect(foldSlices([item('a', 0)])).toEqual([])
    expect(foldSlices([item('a', 5), item('b', 0)]).map((s) => s.key)).toEqual(['a'])
  })

  it('paints neutral keys grey and "Other" its own grey, never a categorical hue', () => {
    const slices = foldSlices([item('(none)', 50), item('hn', 40), item('x', 1), item('y', 1)], {
      neutral: ['(none)'],
      minShare: 0.05,
    })
    expect(slices.find((s) => s.key === '(none)')?.color).toBe(COLOR_UNKNOWN)
    expect(slices.find((s) => s.key === 'other')?.color).toBe(COLOR_PIE_OTHER)
    expect(pieSlotColors).toContain(slices.find((s) => s.key === 'hn')?.color)
  })
})

describe('pieColors', () => {
  it('gives a key the same color whatever its rank or count', () => {
    const a = pieColors(['hn', 'reddit', 'twitter'])
    const b = pieColors(['twitter', 'hn', 'reddit'])
    expect(a).toEqual(b)
  })

  it('never gives two keys in one pie the same color while slots last', () => {
    const keys = pieSlotColors.map((_, i) => `channel-${String(i)}`)
    const colors = pieColors(keys)
    expect(new Set(keys.map((k) => colors.get(k))).size).toBe(keys.length)
  })
})

describe('donutArc', () => {
  it('draws a quarter ring from 12 o’clock clockwise', () => {
    // Outer r=10, inner r=5, centered at (50, 50): starts at the top, ends at 3 o'clock.
    expect(donutArc(0, 0.25, 10, 5, 50, 50)).toBe('M 50 40 A 10 10 0 0 1 60 50 L 55 50 A 5 5 0 0 0 50 45 Z')
  })

  it('uses the large-arc flag past half the ring', () => {
    expect(donutArc(0, 0.75, 10, 5, 50, 50)).toContain('A 10 10 0 1 1')
  })

  it('draws a full ring as two halves, since one SVG arc cannot close on itself', () => {
    const d = donutArc(0, 1, 10, 5, 50, 50)
    expect(d.match(/A 10 10/g)).toHaveLength(2)
    expect(d.match(/A 5 5/g)).toHaveLength(2)
  })
})
