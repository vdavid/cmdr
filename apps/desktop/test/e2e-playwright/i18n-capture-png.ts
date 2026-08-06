/**
 * Blank-screenshot detector for the i18n capture harness: decodes a PNG the
 * native capture just wrote and judges whether it actually carries UI.
 *
 * Why this exists: `page.screenshot()` grabs the window's last COMPOSITED
 * CoreGraphics frame, and macOS doesn't composite a window that isn't frontmost,
 * so a shot of a backgrounded window silently returns the stale empty frame from
 * before the frontend painted. Nothing downstream can tell that apart from a real
 * capture: the DOM was correct, the key dump was full, and the run went green. A
 * whole capture run once shipped 31 blank images that way. So the PIXELS are the
 * contract: `i18n-capture-helpers.ts`'s `shoot()` verifies every written PNG here
 * and refuses to record a surface it can't photograph. ❌ Don't remove this check.
 *
 * Pure Node (no browser, no running app), so `i18n-capture-png.test.ts` unit-tests
 * it directly. Hand-rolled rather than pulling in `sharp`/`pngjs`: the capture
 * writes exactly one PNG flavor (8-bit RGBA, non-interlaced), which is ~60 lines
 * of `zlib.inflateSync` plus the five row filters.
 */

import { inflateSync } from 'node:zlib'

/** A decoded image: RGBA8 pixels, row-major, 4 bytes per pixel. */
export interface DecodedPng {
  width: number
  height: number
  pixels: Buffer
}

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

/**
 * Decodes an 8-bit RGBA non-interlaced PNG (what the native capture writes) into
 * raw pixels. Throws on any other flavor rather than guessing: a silently
 * mis-decoded image would make the blankness verdict meaningless.
 */
export function decodePng(buf: Buffer): DecodedPng {
  if (buf.length < 8 || !buf.subarray(0, 8).equals(PNG_SIGNATURE)) throw new Error('not a PNG (bad signature)')
  let width = 0
  let height = 0
  let depth = 0
  let colorType = 0
  let interlace = 0
  let sawHeader = false
  const idatParts: Buffer[] = []
  let offset = 8
  while (offset + 8 <= buf.length) {
    const length = buf.readUInt32BE(offset)
    const type = buf.toString('ascii', offset + 4, offset + 8)
    const data = buf.subarray(offset + 8, offset + 8 + length)
    if (data.length < length) throw new Error(`truncated ${type} chunk`)
    if (type === 'IHDR') {
      width = data.readUInt32BE(0)
      height = data.readUInt32BE(4)
      depth = data[8]
      colorType = data[9]
      interlace = data[12]
      sawHeader = true
    } else if (type === 'IDAT') {
      idatParts.push(data)
    } else if (type === 'IEND') {
      break
    }
    offset += 12 + length // length + type + data + CRC
  }
  if (!sawHeader) throw new Error('no IHDR chunk')
  if (depth !== 8 || colorType !== 6 || interlace !== 0) {
    throw new Error(
      `unsupported PNG: depth ${String(depth)}, color type ${String(colorType)}, interlace ${String(interlace)}`,
    )
  }
  if (idatParts.length === 0) throw new Error('no IDAT chunks')

  const raw = inflateSync(Buffer.concat(idatParts))
  const bytesPerPixel = 4
  const stride = width * bytesPerPixel
  if (raw.length < height * (stride + 1)) throw new Error('IDAT shorter than the declared image')

  const pixels = Buffer.alloc(height * stride)
  let read = 0
  for (let y = 0; y < height; y++) {
    const filter = raw[read]
    read += 1
    const line = raw.subarray(read, read + stride)
    read += stride
    const cur = pixels.subarray(y * stride, (y + 1) * stride)
    const prevStart = (y - 1) * stride
    switch (filter) {
      case 0: // None
        line.copy(cur)
        break
      case 1: // Sub
        for (let i = 0; i < stride; i++) cur[i] = (line[i] + (i >= bytesPerPixel ? cur[i - bytesPerPixel] : 0)) & 255
        break
      case 2: // Up
        for (let i = 0; i < stride; i++) cur[i] = (line[i] + (y > 0 ? pixels[prevStart + i] : 0)) & 255
        break
      case 3: // Average
        for (let i = 0; i < stride; i++) {
          const left = i >= bytesPerPixel ? cur[i - bytesPerPixel] : 0
          const up = y > 0 ? pixels[prevStart + i] : 0
          cur[i] = (line[i] + ((left + up) >> 1)) & 255
        }
        break
      case 4: // Paeth
        for (let i = 0; i < stride; i++) {
          const left = i >= bytesPerPixel ? cur[i - bytesPerPixel] : 0
          const up = y > 0 ? pixels[prevStart + i] : 0
          const upLeft = y > 0 && i >= bytesPerPixel ? pixels[prevStart + i - bytesPerPixel] : 0
          const estimate = left + up - upLeft
          const dLeft = Math.abs(estimate - left)
          const dUp = Math.abs(estimate - up)
          const dUpLeft = Math.abs(estimate - upLeft)
          const predictor = dLeft <= dUp && dLeft <= dUpLeft ? left : dUp <= dUpLeft ? up : upLeft
          cur[i] = (line[i] + predictor) & 255
        }
        break
      default:
        throw new Error(`unknown row filter ${String(filter)} on row ${String(y)}`)
    }
  }
  return { width, height, pixels }
}

/**
 * Minimum distinct colors (quantized to 5 bits per channel) a real surface shows.
 *
 * Calibrated against a full 133-surface run: every blank shot had exactly 8
 * (window background + the three macOS traffic lights and their antialiasing),
 * while the sparsest REAL surface (the empty transfer-queue window) had 60. The
 * threshold sits an order of magnitude clear of the blanks and 2.5x under the
 * sparsest real surface.
 */
export const MIN_DISTINCT_COLORS = 24

/**
 * Minimum fraction of sampled pixels that must differ from the image's dominant
 * color. Same calibration run: blanks sat at 0.0023 (the traffic lights are all
 * a blank window has), the sparsest real surface at 0.022.
 */
export const MIN_CONTENT_FRACTION = 0.01

/** Sample every Nth pixel in both axes: 4x cheaper, same verdict at these margins. */
const SAMPLE_STEP = 2

/** What the blank check concluded about one written PNG. */
export interface ContentVerdict {
  /** True when the image carries enough distinct content to be a real capture. */
  ok: boolean
  /** Human-readable failure reason; empty when `ok`. */
  reason: string
  /** Distinct 5-bit-per-channel colors found in the sampled pixels. */
  distinctColors: number
  /** Fraction of sampled pixels that differ from the dominant color. */
  contentFraction: number
  width: number
  height: number
}

/**
 * Judges whether `bytes` is a real screenshot or a content-free frame.
 *
 * Two independent signals, both of which must hold, because either alone has a
 * plausible false positive: a page that is genuinely one flat color with a small
 * colorful widget would pass the color count, and a two-tone surface covering
 * half the window would pass the area fraction. Together they say "many colors,
 * over a meaningful area", which is what UI looks like and what an uncomposited
 * window never does.
 *
 * Never throws: an undecodable file is a failed capture, reported as one.
 */
export function assessImageContent(bytes: Buffer): ContentVerdict {
  let image: DecodedPng
  try {
    image = decodePng(bytes)
  } catch (err) {
    return {
      ok: false,
      reason: `could not decode the PNG: ${err instanceof Error ? err.message : String(err)}`,
      distinctColors: 0,
      contentFraction: 0,
      width: 0,
      height: 0,
    }
  }
  const { width, height, pixels } = image
  // Quantize to 5 bits per channel so gradient/antialiasing noise doesn't read as
  // "content": a blank window's traffic lights still collapse to a handful of
  // buckets, while real UI keeps dozens.
  const histogram = new Map<number, number>()
  let sampled = 0
  for (let y = 0; y < height; y += SAMPLE_STEP) {
    const rowStart = y * width * 4
    for (let x = 0; x < width; x += SAMPLE_STEP) {
      const i = rowStart + x * 4
      const bucket = ((pixels[i] >> 3) << 10) | ((pixels[i + 1] >> 3) << 5) | (pixels[i + 2] >> 3)
      histogram.set(bucket, (histogram.get(bucket) ?? 0) + 1)
      sampled += 1
    }
  }
  let dominant = 0
  for (const count of histogram.values()) if (count > dominant) dominant = count
  const distinctColors = histogram.size
  const contentFraction = sampled === 0 ? 0 : 1 - dominant / sampled

  const problems: string[] = []
  if (distinctColors < MIN_DISTINCT_COLORS) {
    problems.push(`only ${String(distinctColors)} distinct colors (need ${String(MIN_DISTINCT_COLORS)})`)
  }
  if (contentFraction < MIN_CONTENT_FRACTION) {
    problems.push(
      `only ${(contentFraction * 100).toFixed(2)}% of pixels differ from the background ` +
        `(need ${(MIN_CONTENT_FRACTION * 100).toFixed(2)}%)`,
    )
  }
  return {
    ok: problems.length === 0,
    reason: problems.length === 0 ? '' : `blank or content-free image: ${problems.join('; ')}`,
    distinctColors,
    contentFraction,
    width,
    height,
  }
}
