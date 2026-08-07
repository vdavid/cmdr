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
 * It also owns the ENCODE side, because the harness crops some surfaces (soft
 * dialogs, toasts, the indexing tiles) to their element bounds after the native
 * capture writes the full window: `@srsholmes/tauri-playwright`'s `screenshot()`
 * takes a path and nothing else, so the framing has to happen on our side, on
 * the bytes.
 *
 * Pure Node (no browser, no running app), so `i18n-capture-png.test.ts` unit-tests
 * it directly. Hand-rolled rather than pulling in `sharp`/`pngjs`: the capture
 * writes exactly one PNG flavor (8-bit RGBA, non-interlaced), which is ~60 lines
 * of `zlib.inflateSync` plus the five row filters.
 */

import { deflateSync, inflateSync } from 'node:zlib'

/** A decoded image: RGBA8 pixels, row-major, 4 bytes per pixel. */
export interface DecodedPng {
  width: number
  height: number
  pixels: Buffer
}

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

/** Bytes per pixel in the one format we accept (RGBA8). */
const BPP = 4

/** The terminating chunk every complete PNG ends with: length 0, type IEND, CRC. */
const IEND_CHUNK = Buffer.from([0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82])

/**
 * Whether `bytes` is a WHOLE PNG file rather than one still being written.
 *
 * The native capture's file write completes AFTER its command returns, so a read
 * that fires immediately can land on a partial file (or nothing at all). Since a
 * PNG always ends with the fixed 12-byte IEND chunk, its presence at the tail is
 * an exact end-of-file marker: no polling on file size, no guessing with a timer.
 * `shoot()` waits on this before it judges an image.
 */
export function isCompletePng(bytes: Buffer): boolean {
  if (bytes.length < PNG_SIGNATURE.length + IEND_CHUNK.length) return false
  if (!bytes.subarray(0, PNG_SIGNATURE.length).equals(PNG_SIGNATURE)) return false
  return bytes.subarray(bytes.length - IEND_CHUNK.length).equals(IEND_CHUNK)
}

/** The header fields we care about, plus the concatenated compressed pixel data. */
interface PngParts {
  width: number
  height: number
  depth: number
  colorType: number
  interlace: number
  idat: Buffer[]
}

/** Walks the chunk stream, collecting IHDR fields and every IDAT payload. */
function parseChunks(buf: Buffer): PngParts {
  if (buf.length < 8 || !buf.subarray(0, 8).equals(PNG_SIGNATURE)) throw new Error('not a PNG (bad signature)')
  const parts: PngParts = { width: 0, height: 0, depth: 0, colorType: 0, interlace: 0, idat: [] }
  let sawHeader = false
  let offset = 8
  while (offset + 8 <= buf.length) {
    const length = buf.readUInt32BE(offset)
    const type = buf.toString('ascii', offset + 4, offset + 8)
    const data = buf.subarray(offset + 8, offset + 8 + length)
    if (data.length < length) throw new Error(`truncated ${type} chunk`)
    if (type === 'IHDR') {
      parts.width = data.readUInt32BE(0)
      parts.height = data.readUInt32BE(4)
      parts.depth = data[8]
      parts.colorType = data[9]
      parts.interlace = data[12]
      sawHeader = true
    } else if (type === 'IDAT') {
      parts.idat.push(data)
    } else if (type === 'IEND') {
      break
    }
    offset += 12 + length // length + type + data + CRC
  }
  if (!sawHeader) throw new Error('no IHDR chunk')
  if (parts.idat.length === 0) throw new Error('no IDAT chunks')
  return parts
}

/** The PNG spec's Paeth predictor: whichever neighbor the gradient points at. */
function paeth(left: number, up: number, upLeft: number): number {
  const estimate = left + up - upLeft
  const dLeft = Math.abs(estimate - left)
  const dUp = Math.abs(estimate - up)
  const dUpLeft = Math.abs(estimate - upLeft)
  if (dLeft <= dUp && dLeft <= dUpLeft) return left
  return dUp <= dUpLeft ? up : upLeft
}

/**
 * Reverses one row's filter in place. `cur` is the row being reconstructed (its
 * already-decoded bytes are the "left" neighbors), `prev` the row above, empty on
 * the first row so the spec's zero-neighbor rule falls out naturally.
 */
function unfilterRow(filter: number, line: Buffer, cur: Buffer, prev: Buffer, rowIndex: number): void {
  const stride = cur.length
  if (filter === 0) {
    line.copy(cur)
    return
  }
  for (let i = 0; i < stride; i++) {
    const left = i >= BPP ? cur[i - BPP] : 0
    const up = prev[i]
    const upLeft = i >= BPP ? prev[i - BPP] : 0
    let predictor: number
    switch (filter) {
      case 1:
        predictor = left
        break
      case 2:
        predictor = up
        break
      case 3:
        predictor = (left + up) >> 1
        break
      case 4:
        predictor = paeth(left, up, upLeft)
        break
      default:
        throw new Error(`unknown row filter ${String(filter)} on row ${String(rowIndex)}`)
    }
    cur[i] = (line[i] + predictor) & 255
  }
}

/**
 * Decodes an 8-bit RGBA non-interlaced PNG (what the native capture writes) into
 * raw pixels. Throws on any other flavor rather than guessing: a silently
 * mis-decoded image would make the blankness verdict meaningless.
 */
export function decodePng(buf: Buffer): DecodedPng {
  const { width, height, depth, colorType, interlace, idat } = parseChunks(buf)
  if (depth !== 8 || colorType !== 6 || interlace !== 0) {
    throw new Error(
      `unsupported PNG: depth ${String(depth)}, color type ${String(colorType)}, interlace ${String(interlace)}`,
    )
  }
  const raw = inflateSync(Buffer.concat(idat))
  const stride = width * BPP
  if (raw.length < height * (stride + 1)) throw new Error('IDAT shorter than the declared image')

  const pixels = Buffer.alloc(height * stride)
  const firstRowNeighbors = Buffer.alloc(stride) // all zeros: the spec's "no row above"
  let read = 0
  for (let y = 0; y < height; y++) {
    const filter = raw[read]
    read += 1
    const line = raw.subarray(read, read + stride)
    read += stride
    const cur = pixels.subarray(y * stride, (y + 1) * stride)
    const prev = y > 0 ? pixels.subarray((y - 1) * stride, y * stride) : firstRowNeighbors
    unfilterRow(filter, line, cur, prev, y)
  }
  return { width, height, pixels }
}

/** CRC-32 (the PNG chunk checksum), so an encoded file is valid to every reader. */
function crc32(buf: Buffer): number {
  let c = 0xffffffff
  for (const byte of buf) {
    c ^= byte
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1
  }
  return (c ^ 0xffffffff) >>> 0
}

/** Wraps a payload in the PNG chunk frame: length, type, data, CRC. */
function encodeChunk(type: string, data: Buffer): Buffer {
  const length = Buffer.alloc(4)
  length.writeUInt32BE(data.length)
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body))
  return Buffer.concat([length, body, crc])
}

/** Applies one row filter, the inverse of `unfilterRow`. */
function filterRow(filter: number, cur: Buffer, prev: Buffer, out: Buffer): void {
  for (let i = 0; i < cur.length; i++) {
    const left = i >= BPP ? cur[i - BPP] : 0
    const up = prev[i]
    const upLeft = i >= BPP ? prev[i - BPP] : 0
    let predictor: number
    switch (filter) {
      case 0:
        predictor = 0
        break
      case 1:
        predictor = left
        break
      case 2:
        predictor = up
        break
      case 3:
        predictor = (left + up) >> 1
        break
      case 4:
        predictor = paeth(left, up, upLeft)
        break
      default:
        throw new Error(`unknown row filter ${String(filter)}`)
    }
    out[i] = (cur[i] - predictor) & 255
  }
}

/**
 * Encodes RGBA8 pixels as an 8-bit RGBA non-interlaced PNG: the same flavor the
 * native capture writes, so a cropped image is byte-compatible with everything
 * downstream (including `decodePng` and the blank check).
 *
 * `filter` picks the per-row filter. The default (0, "None") is what the crop
 * path uses: these images are a few hundred KB and deflate well without it. The
 * other four exist so the decoder's branches can be exercised from a test with
 * real bytes rather than a second hand-rolled encoder.
 */
export function encodePng(width: number, height: number, pixels: Buffer, filter = 0): Buffer {
  const stride = width * BPP
  const raw = Buffer.alloc(height * (stride + 1))
  const zeroRow = Buffer.alloc(stride) // the spec's "no row above" for row 0
  for (let y = 0; y < height; y++) {
    const cur = pixels.subarray(y * stride, (y + 1) * stride)
    const prev = y > 0 ? pixels.subarray((y - 1) * stride, y * stride) : zeroRow
    raw[y * (stride + 1)] = filter
    filterRow(filter, cur, prev, raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)))
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // color type: RGBA
  return Buffer.concat([
    PNG_SIGNATURE,
    encodeChunk('IHDR', ihdr),
    encodeChunk('IDAT', deflateSync(raw)),
    encodeChunk('IEND', Buffer.alloc(0)),
  ])
}

/** A crop window in IMAGE pixels (already scaled by the device pixel ratio). */
export interface CropRect {
  left: number
  top: number
  width: number
  height: number
}

/**
 * The smallest crop worth writing, in image pixels. A rect under this is a
 * measurement gone wrong (a collapsed element, a stale rect), and a 3-pixel PNG
 * helps nobody, so the caller keeps the full window instead.
 */
export const MIN_CROP_SIDE = 16

/**
 * Returns `bytes` cropped to `rect`, re-encoded as the same PNG flavor. The rect
 * is CLAMPED to the image, so a padded rect that runs past an edge yields the
 * edge rather than an error, and returns null when the clamped rect is degenerate
 * (smaller than `MIN_CROP_SIDE` on either side) so the caller can keep the
 * uncropped image rather than ship a sliver.
 */
export function cropPng(bytes: Buffer, rect: CropRect): Buffer | null {
  const image = decodePng(bytes)
  const left = Math.max(0, Math.min(image.width, Math.round(rect.left)))
  const top = Math.max(0, Math.min(image.height, Math.round(rect.top)))
  const right = Math.max(left, Math.min(image.width, Math.round(rect.left + rect.width)))
  const bottom = Math.max(top, Math.min(image.height, Math.round(rect.top + rect.height)))
  const width = right - left
  const height = bottom - top
  if (width < MIN_CROP_SIDE || height < MIN_CROP_SIDE) return null

  const srcStride = image.width * BPP
  const dstStride = width * BPP
  const pixels = Buffer.alloc(height * dstStride)
  for (let y = 0; y < height; y++) {
    const srcStart = (top + y) * srcStride + left * BPP
    image.pixels.copy(pixels, y * dstStride, srcStart, srcStart + dstStride)
  }
  return encodePng(width, height, pixels)
}

/**
 * Minimum distinct colors (quantized to 5 bits per channel) a real surface shows.
 *
 * Calibrated against a full 133-surface run: every blank shot had exactly 8
 * (window background + the three macOS traffic lights and their antialiasing),
 * while the sparsest REAL surface (the empty operation-queue window) had 60. The
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
