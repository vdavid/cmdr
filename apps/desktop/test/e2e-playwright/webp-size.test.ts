import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { webpCanvasSize } from './webp-size.js'

/** Four levels up from `apps/desktop/test/e2e-playwright/`. */
const REPO_ROOT = join(import.meta.dirname, '..', '..', '..', '..')

/** A RIFF/WEBP container carrying one chunk, which is all the header reader looks at. */
function webpContainer(fourcc: string, payload: Buffer): Buffer {
  const chunk = Buffer.concat([
    Buffer.from(fourcc, 'ascii'),
    (() => {
      const size = Buffer.alloc(4)
      size.writeUInt32LE(payload.length)
      return size
    })(),
    payload,
    payload.length % 2 === 1 ? Buffer.from([0]) : Buffer.alloc(0),
  ])
  const riffSize = Buffer.alloc(4)
  riffSize.writeUInt32LE(4 + chunk.length)
  return Buffer.concat([Buffer.from('RIFF', 'ascii'), riffSize, Buffer.from('WEBP', 'ascii'), chunk])
}

/** VP8X: 4 flag bytes, then canvas width-1 and height-1 as 24-bit little-endian. */
function vp8x(width: number, height: number): Buffer {
  const payload = Buffer.alloc(10)
  payload.writeUIntLE(width - 1, 4, 3)
  payload.writeUIntLE(height - 1, 7, 3)
  return webpContainer('VP8X', payload)
}

/** VP8L: a 0x2f signature byte, then 14 bits of width-1 and 14 bits of height-1. */
function vp8l(width: number, height: number): Buffer {
  const bits = BigInt(width - 1) | (BigInt(height - 1) << 14n)
  const payload = Buffer.alloc(6)
  payload[0] = 0x2f
  payload.writeUIntLE(Number(bits & 0xffffffffn), 1, 4)
  return webpContainer('VP8L', payload)
}

describe('webpCanvasSize', () => {
  it('reads the canvas of the committed master, ICC profile and all', () => {
    // The real file is VP8X-wrapped because `magick` attaches an ICC profile, so the
    // synthetic fixtures below can't stand in for it: they'd never catch a reader that
    // only handles a bare VP8L stream.
    const bytes = readFileSync(join(REPO_ROOT, 'brand', 'screenshots', 'app-main-dark.webp'))

    expect(webpCanvasSize(bytes)).toEqual({ width: 2508, height: 1634 })
  })

  it('reads a VP8X canvas', () => {
    expect(webpCanvasSize(vp8x(2508, 1634))).toEqual({ width: 2508, height: 1634 })
  })

  it('reads a lossless VP8L canvas', () => {
    expect(webpCanvasSize(vp8l(640, 480))).toEqual({ width: 640, height: 480 })
  })

  it('refuses anything that is not a WebP, rather than inventing a size', () => {
    expect(() => webpCanvasSize(Buffer.from('not an image at all'))).toThrow(/RIFF\/WEBP/)
    expect(() => webpCanvasSize(webpContainer('VP8 ', Buffer.alloc(16)))).toThrow(/lossy/)
  })
})
