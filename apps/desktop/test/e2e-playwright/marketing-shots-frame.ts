/**
 * Frame arithmetic for the marketing capture: reading a window's rect back out of a
 * `screencapture -l` PNG, judging whether it carries a FOCUSED macOS shadow, and
 * picking the CGWindowID to shoot.
 *
 * Kept apart from the capture itself so every judgement call in here is a pure
 * function with unit tests. The impure half (`osascript`, `screencapture`) lives in
 * `marketing-shots-native.ts` and is proven by real runs.
 */

import { decodePng } from './i18n-capture-png.js'

/**
 * Margins, in device pixels, that macOS leaves around a FOCUSED window's rect when
 * `screencapture -l` writes it onto transparency. The shadow is cast downward, so the
 * bottom margin is nearly twice the top one.
 *
 * The marketing masters are built on these: `app-main` is a 2284x1410 window on a
 * 2508x1634 canvas at +112+76, and the website hero's frame layer is mostly this
 * gradient. Re-derive them by shooting a focused window and reading
 * `magick shot.png -alpha extract -threshold 99% -format '%@' info:`.
 */
export const FOCUSED_SHADOW_X = 112
export const FOCUSED_SHADOW_TOP = 76
export const FOCUSED_SHADOW_BOTTOM = 148

/**
 * How much wider and taller the canvas is than the window it holds. Both axes come to
 * 224 (112 + 112 across, 76 + 148 down), which is what makes one constant enough.
 */
export const FOCUSED_CANVAS_GROWTH = FOCUSED_SHADOW_X * 2

/**
 * The margins an UNFOCUSED window gets, quoted in the failure message only.
 *
 * Not a gate: any margin SMALLER than the focused one means the window lost the front
 * position, and pinning the check to these exact numbers would go quiet the day macOS
 * changes them. They're here so the message can say what the reader is probably
 * looking at.
 */
const UNFOCUSED_SHADOW_HINT = '68/52'

/**
 * Alpha at or above which a pixel counts as part of the window rather than its
 * shadow: ImageMagick's `-threshold 99%` in 8-bit terms, `ceil(0.99 * 255)`.
 *
 * ❗ The exact value is load-bearing, not a round number. The hero compositing reads
 * the window rect out of these same images with `-alpha extract -threshold 99%`, so a
 * looser threshold here would have this pipeline and `regenerate-hero.sh` disagree
 * about where the window ends. Measured on `brand/screenshots/app-main-dark.png`
 * (2026-08-12): 99% gives exactly `2284x1410+112+76`, while 90% gives
 * `2286x1412+111+75` and 1% gives `2465x1592+22+21` — the antialiased corner ring and
 * the shadow's own gradient sit that close to the window's edge.
 */
const OPAQUE_ALPHA = 253

export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

export interface WindowSize {
  width: number
  height: number
}

/** A decoded shot's canvas plus the window rect found inside it. */
interface Framing {
  canvas: WindowSize
  rect: Rect | null
}

/**
 * The smallest rect covering every near-opaque pixel of `bytes`, or null when the
 * image has none.
 *
 * ❗ The threshold is the point. A bounding box over "any alpha above zero" would
 * swallow the shadow halo and report the whole canvas, which would make every check
 * below pass on exactly the images they exist to reject.
 */
export function opaqueBoundingBox(bytes: Buffer, minAlpha: number = OPAQUE_ALPHA): Rect | null {
  return frame(bytes, minAlpha).rect
}

function frame(bytes: Buffer, minAlpha: number): Framing {
  const { width, height, pixels } = decodePng(bytes)
  let minX = width
  let minY = height
  let maxX = -1
  let maxY = -1
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (pixels[(y * width + x) * 4 + 3] < minAlpha) continue
      if (x < minX) minX = x
      if (x > maxX) maxX = x
      if (y < minY) minY = y
      if (y > maxY) maxY = y
    }
  }
  const canvas = { width, height }
  if (maxX < 0) return { canvas, rect: null }
  return { canvas, rect: { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 } }
}

export type FrameVerdict = { ok: true; rect: Rect } | { ok: false; reason: string }

/**
 * Whether `bytes` is a usable marketing master: one window of exactly the staged size,
 * sitting on the focused shadow margins, with the whole shadow inside the canvas.
 *
 * This is the gate the Playwright plugin's own native capture cannot provide, because
 * that one returns the bare window rect with no shadow at all. Everything downstream
 * (the hero's frame layer, and the window offset `regenerate-hero.sh` derives from the
 * alpha) is built on the shadow being real and focused, so a wrong one fails here
 * rather than shipping.
 *
 * `window` is the live window rect in DEVICE pixels, read off the running app rather
 * than hardcoded: the settings window's size tracks the system text scale, so a
 * constant would be right only on the machine it was measured on.
 */
export function verifyShadowFrame(bytes: Buffer, window: WindowSize): FrameVerdict {
  const { canvas, rect } = frame(bytes, OPAQUE_ALPHA)
  if (rect === null) {
    return {
      ok: false,
      reason:
        'the capture has nothing opaque in it, so no window was photographed. ' +
        'Quit or hide whatever app is frontmost, leave the machine alone, and re-run.',
    }
  }
  if (rect.width !== window.width || rect.height !== window.height) {
    return {
      ok: false,
      reason:
        `the photographed window is ${String(rect.width)}x${String(rect.height)}, but the app reports ` +
        `${String(window.width)}x${String(window.height)}. Something resized the window between staging and the shutter.`,
    }
  }
  if (rect.x < FOCUSED_SHADOW_X || rect.y < FOCUSED_SHADOW_TOP) {
    return {
      ok: false,
      reason:
        `the window was not focused when it was shot: its shadow measures ${String(rect.x)}/${String(rect.y)} ` +
        `instead of ${String(FOCUSED_SHADOW_X)}/${String(FOCUSED_SHADOW_TOP)} (an unfocused window typically gives ` +
        `${UNFOCUSED_SHADOW_HINT}). macOS draws the wide shadow only for the key window, and the hero needs it.`,
    }
  }
  if (rect.x !== FOCUSED_SHADOW_X || rect.y !== FOCUSED_SHADOW_TOP) {
    return {
      ok: false,
      reason:
        `the window sits at margin +${String(rect.x)}+${String(rect.y)}, not the focused ` +
        `+${String(FOCUSED_SHADOW_X)}+${String(FOCUSED_SHADOW_TOP)}.`,
    }
  }
  const expectedCanvas = {
    width: window.width + FOCUSED_CANVAS_GROWTH,
    height: window.height + FOCUSED_CANVAS_GROWTH,
  }
  if (canvas.width !== expectedCanvas.width || canvas.height !== expectedCanvas.height) {
    return {
      ok: false,
      reason:
        `the canvas is ${String(canvas.width)}x${String(canvas.height)}, not the ` +
        `${String(expectedCanvas.width)}x${String(expectedCanvas.height)} a focused window's shadow needs. ` +
        'A window pushed against a screen edge has its shadow cropped, so move it fully onto one display and re-run.',
    }
  }
  return { ok: true, rect }
}

/**
 * `rect` pulled in by `inset` device pixels on every side.
 *
 * The hero cutouts use this so the window border and the pane divider stay in the
 * FRAME layer: without it they ride along with a pane as it animates and tear a
 * transparent line down the illustration.
 */
export function insetRect(rect: Rect, inset: number): Rect {
  const width = rect.width - 2 * inset
  const height = rect.height - 2 * inset
  if (width <= 0 || height <= 0) {
    throw new Error(
      `An inset of ${String(inset)} px leaves nothing of a ${String(rect.width)}x${String(rect.height)} rect.`,
    )
  }
  return { x: rect.x + inset, y: rect.y + inset, width, height }
}

/** One entry of the JXA window dump: what `CGWindowListCopyWindowInfo` gives us, trimmed. */
export interface NativeWindow {
  id: number
  x: number
  y: number
  width: number
  height: number
  layer: number
}

/**
 * The CGWindowID to hand `screencapture -l`, or null when nothing matches.
 *
 * Layer 0 is the only layer an app's real windows live on; menus, popovers, and the
 * menu bar sit above and are frequently LARGER than the window we want, so filtering
 * by layer has to come before picking by size.
 *
 * ❗ Matching is by SIZE, never by title. `kCGWindowName` is withheld from a process
 * without Screen Recording permission, so a title filter would silently match nothing
 * on the machine where it matters most. The caller reads the target window's live size
 * off the app and passes it here, which also disambiguates main from settings.
 *
 * `size` is in POINTS, not device pixels: `CGWindowBounds` reports the logical size, so
 * a retina window that photographs 2284x1410 lists here as 1142x705.
 */
export function pickWindowId(windows: NativeWindow[], size?: WindowSize): number | null {
  const ordinary = windows.filter((candidate) => candidate.layer === 0)
  const matching =
    size === undefined
      ? ordinary
      : ordinary.filter((candidate) => candidate.width === size.width && candidate.height === size.height)
  if (matching.length === 0) return null
  const largest = matching.reduce((best, candidate) =>
    candidate.width * candidate.height > best.width * best.height ? candidate : best,
  )
  return largest.id
}
