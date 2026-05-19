import { describe, it, expect } from 'vitest'
import { parseHex, toHex, mixSrgb, withAlpha, relativeLuminance, contrastRatio, readableFgOn } from './srgb-mix'

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

describe('relativeLuminance', () => {
  it('returns 0 for pure black', () => {
    expect(relativeLuminance('#000000')).toBeCloseTo(0, 5)
  })

  it('returns 1 for pure white', () => {
    expect(relativeLuminance('#ffffff')).toBeCloseTo(1, 5)
  })

  it('matches known WCAG values within 4 decimal places', () => {
    // Apple blue and Apple purple, which drive the runtime accent-fg picker.
    expect(relativeLuminance('#087aff')).toBeCloseTo(0.212, 2)
    expect(relativeLuminance('#a54fa7')).toBeCloseTo(0.164, 2)
  })
})

describe('contrastRatio', () => {
  it('returns 21 for black vs white', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 4)
  })

  it('returns 1 for identical colors', () => {
    expect(contrastRatio('#aabbcc', '#aabbcc')).toBeCloseTo(1, 5)
  })

  it('is symmetric in its arguments', () => {
    const a = contrastRatio('#087aff', '#000000')
    const b = contrastRatio('#000000', '#087aff')
    expect(a).toBeCloseTo(b, 6)
  })
})

describe('readableFgOn', () => {
  it('picks black on bright accents (Cmdr gold, yellow, orange)', () => {
    expect(readableFgOn('#d4a006')).toBe('#000000')
    expect(readableFgOn('#ffc601')).toBe('#000000')
    expect(readableFgOn('#f6821b')).toBe('#000000')
  })

  it('picks black on Apple Blue (the macOS default)', () => {
    // Apple Blue is the macOS default. Black wins (5.24:1 vs 4.01:1).
    expect(readableFgOn('#087aff')).toBe('#000000')
  })

  it('picks white on Apple Purple (the only accent where white wins today)', () => {
    // Apple Purple is the dimmest system accent. White wins (4.91:1 vs 4.28:1).
    expect(readableFgOn('#a54fa7')).toBe('#ffffff')
  })

  it('picks black on accents at the crossover (graphite, pink, red)', () => {
    expect(readableFgOn('#8b8c8c')).toBe('#000000')
    expect(readableFgOn('#f74f9f')).toBe('#000000')
    expect(readableFgOn('#ff5157')).toBe('#000000')
  })
})
