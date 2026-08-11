import { describe, expect, it } from 'vitest'
import { decodePng, encodePng } from './i18n-capture-png.js'
import {
  FOCUSED_SHADOW_X,
  FOCUSED_SHADOW_Y,
  UNFOCUSED_SHADOW_X,
  UNFOCUSED_SHADOW_Y,
  insetRect,
  opaqueBoundingBox,
  pickWindowId,
  verifyShadowFrame,
} from './marketing-shots-frame.js'

/**
 * Paints a `canvas`-sized RGBA image whose only opaque area is `rect`, ringed by a
 * faint halo standing in for the macOS shadow. The halo is what makes these tests
 * worth having: a naive "any alpha above zero" bounding box would swallow it and
 * report the whole canvas, which is exactly the mistake that would let an
 * unfocused window pass as a focused one.
 */
function shotWithWindowAt(
  canvas: { width: number; height: number },
  rect: { x: number; y: number; width: number; height: number },
  { haloAlpha = 40 } = {},
): Buffer {
  const pixels = Buffer.alloc(canvas.width * canvas.height * 4)
  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      const at = (y * canvas.width + x) * 4
      const inside = x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
      pixels[at] = inside ? 32 : 0
      pixels[at + 1] = inside ? 32 : 0
      pixels[at + 2] = inside ? 32 : 0
      pixels[at + 3] = inside ? 255 : haloAlpha
    }
  }
  return encodePng(canvas.width, canvas.height, pixels)
}

describe('opaqueBoundingBox', () => {
  it('finds the window rect and ignores the shadow halo around it', () => {
    const bytes = shotWithWindowAt({ width: 200, height: 160 }, { x: 20, y: 30, width: 120, height: 90 })

    expect(opaqueBoundingBox(bytes)).toEqual({ x: 20, y: 30, width: 120, height: 90 })
  })

  it('still finds the rect when the corners are rounded away', () => {
    // A real capture's window corners are transparent, so the corner pixels of the
    // rect carry no alpha at all. The bbox must still be the full rect, because the
    // edges between the corners are opaque.
    const canvas = { width: 100, height: 80 }
    const rect = { x: 10, y: 10, width: 60, height: 40 }
    const bytes = shotWithWindowAt(canvas, rect, { haloAlpha: 0 })
    const rounded = withTransparentCorners(bytes, canvas, rect)

    expect(opaqueBoundingBox(rounded)).toEqual(rect)
  })

  it('returns null for an image with nothing opaque in it', () => {
    const bytes = shotWithWindowAt({ width: 40, height: 40 }, { x: 0, y: 0, width: 0, height: 0 }, { haloAlpha: 12 })

    expect(opaqueBoundingBox(bytes)).toBeNull()
  })
})

describe('verifyShadowFrame', () => {
  const window = { width: 240, height: 180 }
  const canvas = {
    width: window.width + 2 * FOCUSED_SHADOW_X,
    height: window.height + 2 * FOCUSED_SHADOW_Y,
  }

  it('accepts a window sitting at the focused margins', () => {
    const bytes = shotWithWindowAt(canvas, { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_Y, ...window })

    expect(verifyShadowFrame(bytes, window)).toEqual({
      ok: true,
      rect: { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_Y, ...window },
    })
  })

  it('rejects the narrower shadow an unfocused window gets', () => {
    // The whole reason this gate exists: an unfocused window still photographs
    // fine, just with a thinner shadow, and the hero silently loses half its glow.
    const unfocusedCanvas = {
      width: window.width + 2 * UNFOCUSED_SHADOW_X,
      height: window.height + 2 * UNFOCUSED_SHADOW_Y,
    }
    const bytes = shotWithWindowAt(unfocusedCanvas, { x: UNFOCUSED_SHADOW_X, y: UNFOCUSED_SHADOW_Y, ...window })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('not focused')
  })

  it('rejects a window whose size is not the one that was staged', () => {
    const bytes = shotWithWindowAt(canvas, { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_Y, width: 200, height: 180 })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('200x180')
  })

  it('rejects a window shifted off the expected margins', () => {
    const bytes = shotWithWindowAt(canvas, { x: FOCUSED_SHADOW_X + 6, y: FOCUSED_SHADOW_Y, ...window })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('margin')
  })

  it('rejects a capture with no window in it at all', () => {
    const bytes = shotWithWindowAt(canvas, { x: 0, y: 0, width: 0, height: 0 }, { haloAlpha: 0 })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('nothing opaque')
  })
})

describe('insetRect', () => {
  it('shrinks a rect by the inset on every side', () => {
    expect(insetRect({ x: 10, y: 20, width: 100, height: 60 }, 2)).toEqual({ x: 12, y: 22, width: 96, height: 56 })
  })

  it('refuses an inset that would leave nothing behind', () => {
    expect(() => insetRect({ x: 0, y: 0, width: 4, height: 40 }, 2)).toThrow(/inset/i)
  })
})

describe('pickWindowId', () => {
  const windows = [
    { id: 11, width: 80, height: 40, layer: 0, title: 'Cmdr' },
    { id: 12, width: 2284, height: 1410, layer: 0, title: 'Cmdr – Personal use only' },
    { id: 13, width: 4000, height: 3000, layer: 3, title: 'Menubar' },
  ]

  it('picks the largest ordinary window, ignoring higher layers', () => {
    expect(pickWindowId(windows)).toBe(12)
  })

  it('picks by title when one is given, even if it is not the largest', () => {
    expect(pickWindowId(windows, 'Settings')).toBeNull()
    expect(pickWindowId(windows, 'Personal use')).toBe(12)
  })

  it('returns null when the app has no ordinary window yet', () => {
    expect(pickWindowId([{ id: 13, width: 4000, height: 3000, layer: 3, title: 'Menubar' }])).toBeNull()
  })
})

/** Knocks a 3 px triangle out of each corner of `rect`, the way a real window's rounding does. */
function withTransparentCorners(
  bytes: Buffer,
  canvas: { width: number; height: number },
  rect: { x: number; y: number; width: number; height: number },
): Buffer {
  const pixels = Buffer.alloc(canvas.width * canvas.height * 4)
  const decoded = decodeForTest(bytes, canvas)
  decoded.copy(pixels)
  const radius = 3
  for (let dy = 0; dy < radius; dy++) {
    for (let dx = 0; dx < radius - dy; dx++) {
      for (const [cx, cy] of [
        [rect.x + dx, rect.y + dy],
        [rect.x + rect.width - 1 - dx, rect.y + dy],
        [rect.x + dx, rect.y + rect.height - 1 - dy],
        [rect.x + rect.width - 1 - dx, rect.y + rect.height - 1 - dy],
      ]) {
        pixels[(cy * canvas.width + cx) * 4 + 3] = 0
      }
    }
  }
  return encodePng(canvas.width, canvas.height, pixels)
}

/** Re-decodes a freshly encoded fixture; keeps `withTransparentCorners` honest about the real bytes. */
function decodeForTest(bytes: Buffer, canvas: { width: number; height: number }): Buffer {
  const decoded = decodePng(bytes)
  expect(decoded.width).toBe(canvas.width)
  expect(decoded.height).toBe(canvas.height)
  return decoded.pixels
}
