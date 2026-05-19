import { describe, it, expect } from 'vitest'
import { parseHex, toHex, mixSrgb, withAlpha } from './srgb-mix'

describe('parseHex', () => {
  it('parses 6-digit hex', () => {
    expect(parseHex('#ff8040')).toEqual({ r: 255, g: 128, b: 64, a: 1 })
  })

  it('parses 3-digit hex by doubling each nibble', () => {
    expect(parseHex('#fa3')).toEqual({ r: 255, g: 170, b: 51, a: 1 })
  })

  it('parses 8-digit hex with alpha', () => {
    const { a } = parseHex('#00000080')
    expect(a).toBeCloseTo(0x80 / 255, 4)
  })

  it('parses 4-digit hex with alpha', () => {
    const { r, g, b, a } = parseHex('#f00c')
    expect([r, g, b]).toEqual([255, 0, 0])
    expect(a).toBeCloseTo(0xcc / 255, 4)
  })

  it('tolerates surrounding whitespace and missing leading hash', () => {
    expect(parseHex('  abcdef ')).toEqual({ r: 171, g: 205, b: 239, a: 1 })
  })

  it('throws on a malformed string', () => {
    expect(() => parseHex('#zz')).toThrow(/Invalid hex/)
  })
})

describe('toHex', () => {
  it('formats integer channels as zero-padded hex', () => {
    expect(toHex(255, 128, 64)).toBe('#ff8040')
  })

  it('rounds and clamps out-of-range values', () => {
    expect(toHex(-5, 300, 127.6)).toBe('#00ff80')
  })
})

describe('mixSrgb', () => {
  it('returns the first color at t=0', () => {
    expect(mixSrgb('#ff0000', '#00ff00', 0)).toBe('#ff0000')
  })

  it('returns the second color at t=1', () => {
    expect(mixSrgb('#ff0000', '#00ff00', 1)).toBe('#00ff00')
  })

  it('linearly interpolates the channels', () => {
    // 50/50 red+green → (128, 128, 0) after rounding
    expect(mixSrgb('#ff0000', '#00ff00', 0.5)).toBe('#808000')
  })

  it('matches the disk-ok approximation used in app.css', () => {
    // --color-disk-ok (light) = mix #2e7d32 (allow) 60% + #dddddd (border) 40%
    // which is mixSrgb(border, allow, 0.6).
    expect(mixSrgb('#dddddd', '#2e7d32', 0.6)).toBe('#74a376')
  })
})

describe('withAlpha', () => {
  it('returns an rgba(...) string with the given alpha', () => {
    expect(withAlpha('#d4a006', 0.15)).toBe('rgba(212, 160, 6, 0.15)')
  })

  it('rounds channel values', () => {
    expect(withAlpha('#ffffff', 1)).toBe('rgba(255, 255, 255, 1)')
  })
})
