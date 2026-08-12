/**
 * The canvas size of a WebP, read straight from its header.
 *
 * The brand masters are lossless WebP, and nothing in this repo decodes their PIXELS
 * in JavaScript: the capture pipeline gates on the PNG `screencapture` writes, and
 * `regenerate-hero.sh` measures the shipped masters with `magick`. The header alone
 * answers the one question a test can ask everywhere, including a CI runner with no
 * ImageMagick: is the committed master still the size the frame model predicts?
 *
 * Deliberately not a decoder. It reads the container and refuses everything else, so a
 * master that changes format surfaces as a loud failure rather than a silent pass.
 */

interface CanvasSize {
  width: number
  height: number
}

/**
 * Reads the canvas size of a WebP buffer.
 *
 * Handles the two shapes lossless masters come in: `VP8X` (the extended container
 * `magick` writes when it attaches an ICC profile) and a bare `VP8L` stream. Throws on
 * a lossy `VP8 ` frame, which for a master is itself the bug worth reporting.
 */
export function webpCanvasSize(bytes: Buffer): CanvasSize {
  if (bytes.length < 16 || bytes.toString('ascii', 0, 4) !== 'RIFF' || bytes.toString('ascii', 8, 12) !== 'WEBP') {
    throw new Error('not a RIFF/WEBP container')
  }

  const fourcc = bytes.toString('ascii', 12, 16)
  const payload = 20 // 12 container bytes + 4 fourcc + 4 chunk size

  if (fourcc === 'VP8X') {
    // 4 flag bytes, then canvas width-1 and height-1 as 24-bit little-endian.
    if (bytes.length < payload + 10) throw new Error('VP8X chunk is truncated')
    return {
      width: bytes.readUIntLE(payload + 4, 3) + 1,
      height: bytes.readUIntLE(payload + 7, 3) + 1,
    }
  }

  if (fourcc === 'VP8L') {
    // A 0x2f signature byte, then 14 bits of width-1 and 14 bits of height-1, packed
    // least-significant-bit first.
    if (bytes.length < payload + 5) throw new Error('VP8L chunk is truncated')
    if (bytes[payload] !== 0x2f) throw new Error('VP8L chunk has no 0x2f signature')
    const bits = bytes.readUInt32LE(payload + 1)
    return {
      width: (bits & 0x3fff) + 1,
      height: ((bits >>> 14) & 0x3fff) + 1,
    }
  }

  if (fourcc === 'VP8 ') {
    throw new Error('this WebP holds a lossy VP8 frame; masters are lossless')
  }
  throw new Error(`unsupported WebP chunk ${JSON.stringify(fourcc)}`)
}
