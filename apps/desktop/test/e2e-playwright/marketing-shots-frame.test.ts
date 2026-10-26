import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { decodePng, encodePng } from './i18n-capture-png.js'
import { webpCanvasSize } from './webp-size.js'
import {
  insetRect,
  opaqueBoundingBox,
  pickWindowId,
  verifyBareFrame,
  WINDOW_OUTLINE_ALPHA,
  WINDOW_OUTLINE_PX,
  windowOutlineArgs,
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

/**
 * What `screencapture` writes WITHOUT `-o`: the window on a canvas grown by its shadow
 * (112 px each side, 76 above, 148 below, measured on a focused window, 2026-08-12).
 */
function shadowedShotOf(window: { width: number; height: number }): Buffer {
  return shotWithWindowAt({ width: window.width + 224, height: window.height + 224 }, { x: 112, y: 76, ...window })
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
  it('are the window plus its outline, nothing more', () => {
    expect(webpCanvasSize(readFileSync(MASTER))).toEqual({
      width: MASTER_WINDOW.width + 2 * WINDOW_OUTLINE_PX,
      height: MASTER_WINDOW.height + 2 * WINDOW_OUTLINE_PX,
    })
  })

  // 8 s, not the 5 s default: this is the only test here that shells out to ImageMagick and
  // then walks 3.2M pixels in JS. It takes ~0.6 s alone, but a full `pnpm check` run has every
  // core busy and it has measured 6.4 s there, which failed on the default budget.
  it.skipIf(!hasMagick)(
    'hold the window just inside the outline, where the hero script looks for it',
    () => {
      const bytes = masterAsPng()

      expect(opaqueBoundingBox(bytes)).toEqual({ x: WINDOW_OUTLINE_PX, y: WINDOW_OUTLINE_PX, ...MASTER_WINDOW })
      // Transparent beyond the rounded corner, so every page shows its own background there.
      const { width, pixels } = decodePng(bytes)
      expect(pixels[3]).toBe(0)
      expect(pixels[(width - 1) * 4 + 3]).toBe(0)
    },
    8_000,
  )
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

describe('verifyBareFrame', () => {
  const window = { width: 240, height: 180 }

  it('accepts a shadowless window filling its canvas, rounded corners and all', () => {
    const bare = withTransparentCorners(shotWithWindowAt(window, { x: 0, y: 0, ...window }, { haloAlpha: 0 }), window, {
      x: 0,
      y: 0,
      ...window,
    })

    expect(verifyBareFrame(bare, window)).toEqual({ ok: true, rect: { x: 0, y: 0, ...window } })
  })

  it('rejects a capture that still carries a shadow', () => {
    // What `screencapture` gives without `-o`: the window plus transparent margins.
    const verdict = verifyBareFrame(shadowedShotOf(window), window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('shadow')
  })

  it('rejects a window whose size is not the one the app reports', () => {
    const shot = shotWithWindowAt({ width: 200, height: 180 }, { x: 0, y: 0, width: 200, height: 180 })

    const verdict = verifyBareFrame(shot, window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('200x180')
  })

  it('rejects a capture with no window in it at all', () => {
    const verdict = verifyBareFrame(shotWithWindowAt(window, { x: 0, y: 0, width: 0, height: 0 }), window)

    expect(verdict.ok).toBe(false)
    expect(!verdict.ok && verdict.reason).toContain('nothing opaque')
  })
})

describe('windowOutlineArgs', () => {
  it('pads the canvas by the outline width and paints the ring at the theme opacity', () => {
    const args = windowOutlineArgs('dark')

    expect(args).toContain('-border')
    expect(args.join(' ')).toContain(`-evaluate multiply ${String(WINDOW_OUTLINE_ALPHA.dark)}`)
  })

  it('draws a lighter outline in light mode, the way macOS does', () => {
    expect(WINDOW_OUTLINE_ALPHA.light).toBeLessThan(WINDOW_OUTLINE_ALPHA.dark)
  })

  // The real pixels, through the same `magick` the shutter uses: a flat 30-grey "window"
  // must come out with the black outline outside it and, in dark mode, the 2 px white
  // inner line macOS draws just inside the edge, which is what lifts a dark window off a
  // dark page (measured 30 → 53 on the old masters).
  it.skipIf(!hasMagick)('rings a dark window with a black outline and a light inner line', () => {
    const window = { width: 40, height: 30 }
    const out = applyEdges(shotWithWindowAt(window, { x: 0, y: 0, ...window }, { haloAlpha: 0 }), 'dark')
    const { width, pixels } = decodePng(out)
    const at = (x: number, y: number): number[] => [...pixels.subarray((y * width + x) * 4, (y * width + x) * 4 + 4)]

    expect(width).toBe(window.width + 2 * WINDOW_OUTLINE_PX)
    expect(at(0, 15)[3]).toBeGreaterThan(230) // the outline, outside the window
    expect(at(0, 15)[0]).toBe(0)
    for (const x of [1, 2]) expect(at(x, 15)[0]).toBeGreaterThanOrEqual(50) // the inner line
    expect(at(3, 15)[0]).toBe(32) // untouched window from here in
    expect(at(20, 15)).toEqual([32, 32, 32, 255])
  })

  it.skipIf(!hasMagick)('leaves a light window with the outline only', () => {
    const window = { width: 40, height: 30 }
    const out = applyEdges(shotWithWindowAt(window, { x: 0, y: 0, ...window }, { haloAlpha: 0 }), 'light')
    const { width, pixels } = decodePng(out)

    expect(pixels[(15 * width + 1) * 4]).toBe(32)
  })
})

/** Runs `windowOutlineArgs` on PNG bytes through ImageMagick, PNG out. */
function applyEdges(bytes: Buffer, theme: 'dark' | 'light'): Buffer {
  const res = spawnSync('magick', ['png:-', ...windowOutlineArgs(theme), 'png32:-'], {
    input: bytes,
    maxBuffer: 16 * 1024 * 1024,
  })
  if (res.error !== undefined || res.status !== 0) throw new Error(`magick failed: ${String(res.stderr)}`)
  return res.stdout
}

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
