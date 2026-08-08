import { describe, it, expect } from 'vitest'
import { eagerCodePoints, isMeasurable } from './ranges'

describe('isMeasurable', () => {
  it('accepts ordinary code points', () => {
    expect(isMeasurable(0x41)).toBe(true)
    expect(isMeasurable(0x1f600)).toBe(true)
  })

  it('rejects lone surrogates, which String.fromCodePoint throws on', () => {
    expect(isMeasurable(0xd800)).toBe(false)
    expect(isMeasurable(0xdc00)).toBe(false)
    expect(isMeasurable(0xdfff)).toBe(false)
    expect(isMeasurable(0xd7ff)).toBe(true)
    expect(isMeasurable(0xe000)).toBe(true)
  })

  it('rejects out-of-range and non-integer values', () => {
    expect(isMeasurable(-1)).toBe(false)
    expect(isMeasurable(0x110000)).toBe(false)
    expect(isMeasurable(1.5)).toBe(false)
    expect(isMeasurable(Number.NaN)).toBe(false)
  })
})

describe('eagerCodePoints', () => {
  const codePoints = eagerCodePoints()

  it('is strictly ascending, so it is deduplicated by construction', () => {
    for (let i = 1; i < codePoints.length; i++) {
      expect(codePoints[i]).toBeGreaterThan(codePoints[i - 1])
    }
  })

  it('contains every code point a Latin-script filename needs', () => {
    const has = (cp: number) => codePoints.includes(cp)
    expect(has('a'.codePointAt(0) ?? 0)).toBe(true)
    expect(has('Z'.codePointAt(0) ?? 0)).toBe(true)
    expect(has('.'.codePointAt(0) ?? 0)).toBe(true)
    expect(has(' '.codePointAt(0) ?? 0)).toBe(true)
    expect(has('é'.codePointAt(0) ?? 0)).toBe(true)
    expect(has('ß'.codePointAt(0) ?? 0)).toBe(true)
    // macOS stores filenames NFD, so combining marks show up constantly.
    expect(has(0x0301)).toBe(true)
    expect(has('Я'.codePointAt(0) ?? 0)).toBe(true)
    expect(has('π'.codePointAt(0) ?? 0)).toBe(true)
  })

  it('leaves the bulk scripts to the on-demand fill-in', () => {
    const has = (cp: number) => codePoints.includes(cp)
    expect(has(0x4e00)).toBe(false) // CJK Unified Ideographs
    expect(has(0x3400)).toBe(false) // CJK Extension A
    expect(has(0xac00)).toBe(false) // Hangul Syllables
    expect(has(0x1200)).toBe(false) // Ethiopic
    expect(has(0x1000)).toBe(false) // Myanmar
    expect(has(0xa000)).toBe(false) // Yi Syllables
  })

  it('excludes the Private Use Area, which has no standard glyphs', () => {
    expect(codePoints.includes(0xe000)).toBe(false)
    expect(codePoints.includes(0xf8ff)).toBe(false)
  })

  it('stays small enough to measure in one go', () => {
    // The whole point of the split: this used to be ~54,500 code points, which
    // took seconds to measure even on an idle machine. If a future edit re-adds
    // a bulk block, this fails rather than quietly restoring the freeze.
    expect(codePoints.length).toBeGreaterThan(2_000)
    expect(codePoints.length).toBeLessThan(8_000)
  })
})
