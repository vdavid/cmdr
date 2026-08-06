import { describe, it, expect } from 'vitest'
import { deflateSync } from 'node:zlib'
import { decodePng, assessImageContent, MIN_DISTINCT_COLORS, MIN_CONTENT_FRACTION } from './i18n-capture-png.js'

/** CRC-32 (the PNG chunk checksum), so the fixtures below are real PNG bytes. */
function crc32(buf: Buffer): number {
  let c = 0xffffffff
  for (const byte of buf) {
    c ^= byte
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1
  }
  return (c ^ 0xffffffff) >>> 0
}

function chunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body))
  return Buffer.concat([len, body, crc])
}

/**
 * Encodes an 8-bit RGBA non-interlaced PNG, the exact shape the native capture
 * writes. `filterType` picks the per-row filter so the decoder's five branches
 * are all exercised (filter 0 stores raw bytes; the others are re-derived here
 * the same way the spec defines them).
 */
function encodePng(width: number, height: number, pixels: Buffer, filterType = 0): Buffer {
  const stride = width * 4
  const rows: Buffer[] = []
  for (let y = 0; y < height; y++) {
    const cur = pixels.subarray(y * stride, (y + 1) * stride)
    const prev = y > 0 ? pixels.subarray((y - 1) * stride, y * stride) : Buffer.alloc(stride)
    const line = Buffer.alloc(stride)
    for (let i = 0; i < stride; i++) {
      const a = i >= 4 ? cur[i - 4] : 0
      const b = prev[i]
      const c = i >= 4 ? prev[i - 4] : 0
      let predictor = 0
      if (filterType === 1) predictor = a
      else if (filterType === 2) predictor = b
      else if (filterType === 3) predictor = (a + b) >> 1
      else if (filterType === 4) {
        const p = a + b - c
        const pa = Math.abs(p - a)
        const pb = Math.abs(p - b)
        const pc = Math.abs(p - c)
        predictor = pa <= pb && pa <= pc ? a : pb <= pc ? b : c
      }
      line[i] = (cur[i] - predictor) & 255
    }
    rows.push(Buffer.concat([Buffer.from([filterType]), line]))
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // color type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.concat(rows))),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

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
