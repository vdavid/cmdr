import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { decodePng, encodePng } from './i18n-capture-png.js'
import { webpCanvasSize } from './webp-size.js'
import {
  FOCUSED_CANVAS_GROWTH,
  FOCUSED_SHADOW_BOTTOM,
  FOCUSED_SHADOW_TOP,
  FOCUSED_SHADOW_X,
  insetRect,
  opaqueBoundingBox,
  pickWindowId,
  verifyShadowFrame,
} from './marketing-shots-frame.js'

/** The committed master, four levels up from `apps/desktop/test/e2e-playwright/`. */
const REPO_ROOT = join(import.meta.dirname, '..', '..', '..', '..')

/**
 * Paints a `canvas`-sized RGBA image whose only opaque area is `rect`, ringed by a
 * faint halo standing in for the macOS shadow. The halo is what makes these tests
 * worth having: a naive "any alpha above zero" bounding box would swallow it and
 * report the whole canvas, which is exactly the mistake that would let an unfocused
 * window pass as a focused one.
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

/** A well-formed focused shot of a `window`-sized window, on the canvas macOS would give it. */
function focusedShotOf(window: { width: number; height: number }): Buffer {
  return shotWithWindowAt(
    { width: window.width + FOCUSED_CANVAS_GROWTH, height: window.height + FOCUSED_CANVAS_GROWTH },
    { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_TOP, ...window },
  )
}

/** The window inside the master, which every margin here is measured against. */
const MASTER_WINDOW = { width: 2284, height: 1410 }

/** The lossless master. The shutter gates on PNG; only the committed FILE is WebP. */
const MASTER = join(REPO_ROOT, 'brand', 'screenshots', 'app-main-dark.webp')

/**
 * Renders the master to PNG bytes through ImageMagick, the same tool that wrote it and
 * that `regenerate-hero.sh` measures it with. Lossless in, lossless out, so the pixels
 * the gates see are the pixels `screencapture` produced.
 */
function masterAsPng(): Buffer {
  const res = spawnSync('magick', [MASTER, 'png32:-'], { maxBuffer: 256 * 1024 * 1024 })
  if (res.error !== undefined || res.status !== 0) {
    throw new Error(`Rendering the master to PNG failed (\`magick\`). ${String(res.stderr)}`)
  }
  return res.stdout
}

/** ImageMagick isn't an npm dep, so a machine without it runs the header assertions only. */
const hasMagick = spawnSync('magick', ['-version']).status === 0

describe('the committed masters', () => {
  // Anchoring on a real capture rather than only on painted fixtures: these numbers
  // come from macOS, and a model of them that drifts from the pixels would let every
  // synthetic test pass while the pipeline rejected every good shot.
  //
  // Split in two because the master is WebP and nothing here decodes those pixels in
  // JavaScript. The size assertion reads the container header, so it runs everywhere
  // (including CI, which has no ImageMagick) and still catches the likeliest drift: a
  // reshoot at a different size while the constants stay put.
  it('are sized exactly as the focused-frame model predicts', () => {
    expect(webpCanvasSize(readFileSync(MASTER))).toEqual({
      width: MASTER_WINDOW.width + FOCUSED_CANVAS_GROWTH,
      height: MASTER_WINDOW.height + FOCUSED_CANVAS_GROWTH,
    })
  })

  it.skipIf(!hasMagick)('measure exactly the focused margins the pipeline gates on', () => {
    const bytes = masterAsPng()
    const decoded = decodePng(bytes)

    const rect = opaqueBoundingBox(bytes)

    expect(rect).toEqual({ x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_TOP, ...MASTER_WINDOW })
    expect(decoded.width).toBe(MASTER_WINDOW.width + FOCUSED_CANVAS_GROWTH)
    expect(decoded.height).toBe(MASTER_WINDOW.height + FOCUSED_CANVAS_GROWTH)
    expect(verifyShadowFrame(bytes, MASTER_WINDOW)).toEqual({ ok: true, rect })
  })

  it('grow by the same amount on both axes, which is what one constant assumes', () => {
    expect(FOCUSED_SHADOW_X * 2).toBe(FOCUSED_SHADOW_TOP + FOCUSED_SHADOW_BOTTOM)
    expect(FOCUSED_CANVAS_GROWTH).toBe(FOCUSED_SHADOW_TOP + FOCUSED_SHADOW_BOTTOM)
  })
})

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
    const rounded = withTransparentCorners(shotWithWindowAt(canvas, rect, { haloAlpha: 0 }), canvas, rect)

    expect(opaqueBoundingBox(rounded)).toEqual(rect)
  })

  it('returns null for an image with nothing opaque in it', () => {
    const bytes = shotWithWindowAt({ width: 40, height: 40 }, { x: 0, y: 0, width: 0, height: 0 }, { haloAlpha: 12 })

    expect(opaqueBoundingBox(bytes)).toBeNull()
  })
})

describe('verifyShadowFrame', () => {
  const window = { width: 240, height: 180 }
  const canvas = { width: window.width + FOCUSED_CANVAS_GROWTH, height: window.height + FOCUSED_CANVAS_GROWTH }

  it('accepts a window sitting at the focused margins', () => {
    expect(verifyShadowFrame(focusedShotOf(window), window)).toEqual({
      ok: true,
      rect: { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_TOP, ...window },
    })
  })

  it('rejects the narrower shadow an unfocused window gets', () => {
    // The whole reason this gate exists: an unfocused window still photographs fine,
    // just with a thinner shadow, and the hero silently loses half its glow.
    const bytes = shotWithWindowAt(
      { width: window.width + 2 * 68, height: window.height + 2 * 52 },
      { x: 68, y: 52, ...window },
    )

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('not focused')
  })

  it('rejects a window whose size is not the one the app reports', () => {
    const bytes = shotWithWindowAt(canvas, { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_TOP, width: 200, height: 180 })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('200x180')
  })

  it('rejects a window shifted off the expected margins', () => {
    const bytes = shotWithWindowAt(canvas, { x: FOCUSED_SHADOW_X + 6, y: FOCUSED_SHADOW_TOP, ...window })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('margin')
  })

  it('rejects a canvas too small to hold the whole shadow', () => {
    // What a window pushed against a screen edge produces: right margins and a right
    // window size, but the shadow runs off the bottom of the image.
    const cropped = { width: canvas.width, height: canvas.height - 30 }
    const bytes = shotWithWindowAt(cropped, { x: FOCUSED_SHADOW_X, y: FOCUSED_SHADOW_TOP, ...window })

    const verdict = verifyShadowFrame(bytes, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('canvas')
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
    { id: 11, x: 0, y: 0, width: 480, height: 300, layer: 0 },
    { id: 12, x: 40, y: 60, width: 1142, height: 705, layer: 0 },
    { id: 13, x: 0, y: 0, width: 2000, height: 1500, layer: 3 },
  ]

  it('picks the largest ordinary window, ignoring higher layers', () => {
    // The menu bar and popovers live above layer 0 and are often bigger than the
    // window we want, so filtering by layer has to come before picking by size.
    expect(pickWindowId(windows)).toBe(12)
  })

  it('picks by point size when one is given, so settings never resolves to main', () => {
    expect(pickWindowId(windows, { width: 480, height: 300 })).toBe(11)
    expect(pickWindowId(windows, { width: 852, height: 601 })).toBeNull()
  })

  it('returns null when the app has no ordinary window yet', () => {
    expect(pickWindowId([{ id: 13, x: 0, y: 0, width: 2000, height: 1500, layer: 3 }])).toBeNull()
  })
})

/** Knocks a 3 px triangle out of each corner of `rect`, the way a real window's rounding does. */
function withTransparentCorners(
  bytes: Buffer,
  canvas: { width: number; height: number },
  rect: { x: number; y: number; width: number; height: number },
): Buffer {
  const decoded = decodePng(bytes)
  expect(decoded.width).toBe(canvas.width)
  expect(decoded.height).toBe(canvas.height)
  const pixels = Buffer.from(decoded.pixels)
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
