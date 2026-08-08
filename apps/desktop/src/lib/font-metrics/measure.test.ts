import { describe, it, expect, vi } from 'vitest'
import { parseFontId, fontShorthand, measureCodePoints, measureCodePointsChunked, type TextMeasurer } from './measure'

/** Stands in for a Canvas 2D context; reports one pixel per UTF-16 unit. */
function stubMeasurer(): TextMeasurer & { seenFont: string } {
  return {
    font: '',
    seenFont: '',
    measureText(text: string) {
      this.seenFont = this.font
      return { width: text.length }
    },
  }
}

describe('parseFontId', () => {
  it('resolves the system family to the real stack', () => {
    expect(parseFontId('system-400-12')).toEqual({
      fontFamily: '-apple-system, BlinkMacSystemFont, system-ui, sans-serif',
      fontWeight: 400,
      fontSize: 12,
    })
  })

  it('keeps a non-system family verbatim', () => {
    expect(parseFontId('Menlo-600-15').fontFamily).toBe('Menlo')
  })

  it('tracks the size component, which is what varies with the text scale', () => {
    expect(parseFontId('system-400-11').fontSize).toBe(11)
    expect(parseFontId('system-400-24').fontSize).toBe(24)
  })

  it('falls back per component rather than throwing on a malformed ID', () => {
    // A throw here would leave every Brief column unsized; a default won't.
    expect(parseFontId('')).toEqual({
      fontFamily: '-apple-system, BlinkMacSystemFont, system-ui, sans-serif',
      fontWeight: 400,
      fontSize: 12,
    })
  })
})

describe('fontShorthand', () => {
  it('builds the Canvas font string weight-size-family', () => {
    expect(fontShorthand({ fontFamily: 'Menlo', fontWeight: 600, fontSize: 15 })).toBe('600 15px Menlo')
  })
})

describe('measureCodePoints', () => {
  const spec = { fontFamily: 'Menlo', fontWeight: 400, fontSize: 12 }

  it('returns one width per code point, in order', () => {
    const ctx = stubMeasurer()
    const widths = measureCodePoints(ctx, spec, new Uint32Array([0x41, 0x42, 0x43]))

    expect(Array.from(widths)).toEqual([1, 1, 1])
  })

  it('sets the font before measuring', () => {
    const ctx = stubMeasurer()
    measureCodePoints(ctx, spec, new Uint32Array([0x41]))

    expect(ctx.seenFont).toBe('400 12px Menlo')
  })

  it('measures astral code points as one character, not as surrogate halves', () => {
    const ctx = stubMeasurer()
    // The stub reports UTF-16 length, so an emoji comes back as 2: proof the
    // whole code point was passed through rather than being split.
    const widths = measureCodePoints(ctx, spec, new Uint32Array([0x1f600]))

    expect(widths[0]).toBe(2)
  })

  it('returns an empty result for an empty request', () => {
    const ctx = stubMeasurer()
    expect(measureCodePoints(ctx, spec, new Uint32Array([])).length).toBe(0)
  })
})

describe('measureCodePointsChunked', () => {
  const spec = { fontFamily: 'Menlo', fontWeight: 400, fontSize: 12 }

  /** A measurer where every call costs `costMs` of wall clock. */
  function slowMeasurer(costMs: number): TextMeasurer {
    let now = 0
    vi.spyOn(performance, 'now').mockImplementation(() => now)
    return {
      font: '',
      measureText(text: string) {
        now += costMs
        return { width: text.length }
      },
    }
  }

  it('produces the same widths as the unchunked loop', async () => {
    const codePoints = new Uint32Array([0x41, 0x42, 0x43, 0x44])
    const expected = measureCodePoints(stubMeasurer(), spec, codePoints)

    const actual = await measureCodePointsChunked(stubMeasurer(), spec, codePoints, 8, () => Promise.resolve())

    expect(Array.from(actual)).toEqual(Array.from(expected))
  })

  it('yields between slices instead of running the whole loop at once', async () => {
    // This is the property that stops the freeze: 10 code points at 4ms each
    // against an 8ms budget must break into several turns.
    const yieldToEventLoop = vi.fn(() => Promise.resolve())
    const codePoints = new Uint32Array(Array.from({ length: 10 }, (_, i) => 0x41 + i))

    await measureCodePointsChunked(slowMeasurer(4), spec, codePoints, 8, yieldToEventLoop)

    expect(yieldToEventLoop.mock.calls.length).toBeGreaterThan(1)
  })

  it('never yields after the last code point', async () => {
    const yieldToEventLoop = vi.fn(() => Promise.resolve())

    await measureCodePointsChunked(slowMeasurer(100), spec, new Uint32Array([0x41]), 8, yieldToEventLoop)

    expect(yieldToEventLoop).not.toHaveBeenCalled()
  })

  it('always makes progress, even when one measurement blows the whole budget', async () => {
    // A pathologically slow `measureText` must not stall the loop at index 0.
    const yieldToEventLoop = vi.fn(() => Promise.resolve())
    const codePoints = new Uint32Array([0x41, 0x42, 0x43])

    const widths = await measureCodePointsChunked(slowMeasurer(1_000), spec, codePoints, 8, yieldToEventLoop)

    expect(widths.length).toBe(3)
    expect(Array.from(widths)).toEqual([1, 1, 1])
    expect(yieldToEventLoop).toHaveBeenCalledTimes(2)
  })

  it('returns an empty result without yielding for an empty request', async () => {
    const yieldToEventLoop = vi.fn(() => Promise.resolve())

    const widths = await measureCodePointsChunked(stubMeasurer(), spec, new Uint32Array([]), 8, yieldToEventLoop)

    expect(widths.length).toBe(0)
    expect(yieldToEventLoop).not.toHaveBeenCalled()
  })
})
