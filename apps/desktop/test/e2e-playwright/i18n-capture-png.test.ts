import { describe, it, expect } from 'vitest'
import {
  decodePng,
  encodePng,
  cropPng,
  isCompletePng,
  assessImageContent,
  MIN_CROP_SIDE,
  MIN_DISTINCT_COLORS,
  MIN_CONTENT_FRACTION,
} from './i18n-capture-png.js'

/** An all-one-color canvas: what a never-composited window looks like. */
function solid(width: number, height: number, rgb: [number, number, number]): Buffer {
  const pixels = Buffer.alloc(width * height * 4)
  for (let i = 0; i < width * height; i++) {
    pixels[i * 4] = rgb[0]
    pixels[i * 4 + 1] = rgb[1]
    pixels[i * 4 + 2] = rgb[2]
    pixels[i * 4 + 3] = 255
  }
  return pixels
}

describe('decodePng', () => {
  it('round-trips pixels through every row filter', () => {
    const width = 9
    const height = 7
    const pixels = Buffer.alloc(width * height * 4)
    for (let i = 0; i < width * height; i++) {
      pixels[i * 4] = (i * 7) & 255
      pixels[i * 4 + 1] = (i * 13) & 255
      pixels[i * 4 + 2] = (i * 29) & 255
      pixels[i * 4 + 3] = 255
    }
    for (const filterType of [0, 1, 2, 3, 4]) {
      const decoded = decodePng(encodePng(width, height, pixels, filterType))
      expect(decoded.width, `filter ${String(filterType)}`).toBe(width)
      expect(decoded.height, `filter ${String(filterType)}`).toBe(height)
      expect(decoded.pixels.equals(pixels), `filter ${String(filterType)}`).toBe(true)
    }
  })

  it('rejects a non-PNG buffer instead of returning garbage', () => {
    expect(() => decodePng(Buffer.from('not a png at all'))).toThrow()
  })
})

describe('isCompletePng', () => {
  // The native capture's file write lands AFTER its command returns, so the
  // harness watches for a whole file rather than reading whatever is there. A
  // half-written PNG must never reach the content check.
  const whole = encodePng(40, 30, solid(40, 30, [30, 30, 34]))

  it('accepts a fully-written file', () => {
    expect(isCompletePng(whole)).toBe(true)
  })

  it('rejects a file still missing its IEND chunk', () => {
    expect(isCompletePng(whole.subarray(0, whole.length - 12))).toBe(false)
  })

  it('rejects a file cut off mid-IDAT', () => {
    expect(isCompletePng(whole.subarray(0, Math.floor(whole.length / 2)))).toBe(false)
  })

  it('rejects an empty or tiny file', () => {
    expect(isCompletePng(Buffer.alloc(0))).toBe(false)
    expect(isCompletePng(Buffer.from([0x89, 0x50]))).toBe(false)
  })
})

describe('cropPng', () => {
  /** A canvas whose every pixel encodes its own (x, y), so a crop is verifiable. */
  function coordinates(width: number, height: number): Buffer {
    const pixels = Buffer.alloc(width * height * 4)
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const i = (y * width + x) * 4
        pixels[i] = x & 255
        pixels[i + 1] = y & 255
        pixels[i + 2] = (x + y) & 255
        pixels[i + 3] = 255
      }
    }
    return pixels
  }

  it('returns exactly the requested window of pixels', () => {
    const source = encodePng(80, 60, coordinates(80, 60))
    const cropped = cropPng(source, { left: 20, top: 17, width: 40, height: 30 })
    expect(cropped).not.toBeNull()
    const decoded = decodePng(cropped as Buffer)
    expect(decoded.width).toBe(40)
    expect(decoded.height).toBe(30)
    for (const [x, y] of [
      [0, 0],
      [39, 29],
      [21, 12],
    ]) {
      const i = (y * 40 + x) * 4
      expect([decoded.pixels[i], decoded.pixels[i + 1]], `pixel ${String(x)},${String(y)}`).toEqual([
        (20 + x) & 255,
        (17 + y) & 255,
      ])
    }
  })

  it('clamps a rect that runs past the edges instead of failing', () => {
    // The caller pads the element rect so shadows and borders survive, which
    // routinely pushes it outside the window on a dialog near an edge.
    const source = encodePng(50, 40, coordinates(50, 40))
    const cropped = cropPng(source, { left: -10, top: -10, width: 100, height: 100 })
    const decoded = decodePng(cropped as Buffer)
    expect(decoded.width).toBe(50)
    expect(decoded.height).toBe(40)
  })

  it('refuses a degenerate rect rather than writing a sliver', () => {
    const source = encodePng(50, 40, coordinates(50, 40))
    expect(cropPng(source, { left: 0, top: 0, width: MIN_CROP_SIDE - 1, height: 30 })).toBeNull()
    expect(cropPng(source, { left: 48, top: 0, width: 40, height: 30 })).toBeNull() // clamps to 2px wide
  })

  it('produces a whole PNG the blank check still accepts', () => {
    // A crop is what a translator ends up looking at, so it has to survive the
    // same pipeline as a full capture: complete file, decodable, real content.
    const width = 300
    const height = 200
    const pixels = coordinates(width, height)
    const cropped = cropPng(encodePng(width, height, pixels), { left: 10, top: 10, width: 200, height: 150 })
    expect(isCompletePng(cropped as Buffer)).toBe(true)
    expect(assessImageContent(cropped as Buffer).ok).toBe(true)
  })
})

describe('assessImageContent', () => {
  it('rejects a uniform image', () => {
    const verdict = assessImageContent(encodePng(200, 200, solid(200, 200, [30, 30, 34])))
    expect(verdict.ok).toBe(false)
    expect(verdict.distinctColors).toBe(1)
    expect(verdict.contentFraction).toBe(0)
    expect(verdict.reason).not.toBe('')
  })

  it('rejects a near-uniform image: an empty window with only its traffic lights', () => {
    // The real defect's signature: a dark window whose only non-background pixels
    // are the three macOS traffic lights (~0.2% of the canvas, three hues).
    const width = 400
    const height = 300
    const pixels = solid(width, height, [30, 30, 34])
    const lights: [number, number, number][] = [
      [237, 106, 94],
      [245, 191, 79],
      [98, 197, 84],
    ]
    lights.forEach(([r, g, b], index) => {
      for (let y = 6; y < 14; y++) {
        for (let x = 8 + index * 14; x < 16 + index * 14; x++) {
          const i = (y * width + x) * 4
          pixels[i] = r
          pixels[i + 1] = g
          pixels[i + 2] = b
        }
      }
    })
    const verdict = assessImageContent(encodePng(width, height, pixels))
    expect(verdict.ok).toBe(false)
    expect(verdict.contentFraction).toBeLessThan(MIN_CONTENT_FRACTION)
    expect(verdict.distinctColors).toBeLessThan(MIN_DISTINCT_COLORS)
  })

  it('accepts an image carrying real UI content', () => {
    // A window with text-like detail spread over it: many hues, well past a few
    // percent of the canvas.
    const width = 400
    const height = 300
    const pixels = solid(width, height, [30, 30, 34])
    for (let y = 20; y < 280; y += 3) {
      for (let x = 10; x < 390; x++) {
        const i = (y * width + x) * 4
        pixels[i] = (x * 3 + y) & 255
        pixels[i + 1] = (x + y * 5) & 255
        pixels[i + 2] = (x * 7 + y * 2) & 255
      }
    }
    const verdict = assessImageContent(encodePng(width, height, pixels))
    expect(verdict.ok).toBe(true)
    expect(verdict.reason).toBe('')
    expect(verdict.distinctColors).toBeGreaterThanOrEqual(MIN_DISTINCT_COLORS)
    expect(verdict.contentFraction).toBeGreaterThanOrEqual(MIN_CONTENT_FRACTION)
  })

  it('reports a corrupt file as not-ok rather than throwing', () => {
    const verdict = assessImageContent(Buffer.from('truncated garbage'))
    expect(verdict.ok).toBe(false)
    expect(verdict.reason).toMatch(/could not decode/i)
  })
})
