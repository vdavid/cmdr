import { describe, it, expect } from 'vitest'
import { COLOR_UNKNOWN } from './colors.js'
import { referrerSlices } from './referrer-slices.js'

describe('referrerSlices', () => {
  it('turns Umami referrer rows into pie slices keyed and labeled by domain', () => {
    const slices = referrerSlices([
      { x: 'google.com', y: 60 },
      { x: 'reddit.com', y: 40 },
    ])
    expect(slices.map((s) => [s.key, s.label, s.count, s.share])).toEqual([
      ['google.com', 'google.com', 60, 0.6],
      ['reddit.com', 'reddit.com', 40, 0.4],
    ])
  })

  it('names an empty referrer "(direct)" and paints it the unknown grey without using a hue', () => {
    const slices = referrerSlices([
      { x: 'a.com', y: 30 },
      { x: '', y: 25 },
      { x: 'b.com', y: 15 },
      { x: 'c.com', y: 15 },
      { x: 'd.com', y: 15 },
    ])
    const direct = slices.find((s) => s.label === '(direct)')
    expect(direct?.color).toBe(COLOR_UNKNOWN)
    expect(slices.map((s) => s.label)).toEqual(['a.com', '(direct)', 'b.com', 'c.com', 'd.com'])
  })

  it('folds the long tail into "Other" but keeps a lone would-be fold named', () => {
    const tail = referrerSlices([
      { x: 'a.com', y: 50 },
      { x: 'b.com', y: 20 },
      { x: 'c.com', y: 10 },
      { x: 'd.com', y: 10 },
      { x: 'e.com', y: 5 },
      { x: 'f.com', y: 5 },
    ])
    expect(tail.map((s) => s.label)).toEqual(['a.com', 'b.com', 'c.com', 'd.com', 'Other'])
    expect(tail.at(-1)?.folded?.map((f) => f.label)).toEqual(['e.com', 'f.com'])

    const lone = referrerSlices([
      { x: 'a.com', y: 98 },
      { x: 'b.com', y: 2 },
    ])
    expect(lone.map((s) => s.label)).toEqual(['a.com', 'b.com'])
  })

  it('gives the same domain the same color whatever its rank', () => {
    const first = referrerSlices([
      { x: 'google.com', y: 60 },
      { x: 'github.com', y: 40 },
    ])
    const flipped = referrerSlices([
      { x: 'google.com', y: 40 },
      { x: 'github.com', y: 60 },
    ])
    const color = (slices: typeof first, key: string) => slices.find((s) => s.key === key)?.color
    expect(color(first, 'google.com')).toBe(color(flipped, 'google.com'))
    expect(color(first, 'github.com')).toBe(color(flipped, 'github.com'))
  })

  it("merges a site's www., m., l., lm., and mobile. hosts into one slice that lists what it combines", () => {
    const slices = referrerSlices([
      { x: 'm.facebook.com', y: 11 },
      { x: 'l.facebook.com', y: 9 },
      { x: 'facebook.com', y: 3 },
      { x: 'lm.facebook.com', y: 2 },
      { x: 'www.google.com', y: 20 },
      { x: 'mobile.twitter.com', y: 5 },
    ])
    expect(slices.map((s) => [s.key, s.count])).toEqual([
      ['facebook.com', 25],
      ['google.com', 20],
      ['twitter.com', 5],
    ])
    expect(slices[0].members?.map((m) => [m.label, m.count])).toEqual([
      ['m.facebook.com', 11],
      ['l.facebook.com', 9],
      ['facebook.com', 3],
      ['lm.facebook.com', 2],
    ])
    // One host behind a slice: nothing to list, and the label is the bare domain.
    expect(slices[1].label).toBe('google.com')
    expect(slices[1].members).toBeUndefined()
  })

  it('leaves hosts alone when stripping would leave no registrable domain', () => {
    const slices = referrerSlices([
      { x: 'm.com', y: 5 },
      { x: 'l.co', y: 5 },
      { x: 'com.linkedin.android', y: 5 },
    ])
    expect(slices.map((s) => s.key).sort()).toEqual(['com.linkedin.android', 'l.co', 'm.com'])
  })

  it('returns no slices when no visitor came from another site', () => {
    expect(referrerSlices([])).toEqual([])
    expect(referrerSlices([{ x: 'a.com', y: 0 }])).toEqual([])
  })
})
