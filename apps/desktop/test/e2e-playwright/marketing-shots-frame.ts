/**
 * Frame arithmetic for the marketing capture: reading a window's rect back out of a
 * `screencapture -l` PNG, judging whether it carries a FOCUSED macOS shadow, and
 * picking the CGWindowID to shoot.
 *
 * Kept apart from the capture itself so every judgement call in here is a pure
 * function with unit tests. The impure half (`osascript`, `screencapture`) lives in
 * `marketing-shots-helpers.ts` and is proven by real runs.
 */

import { decodePng } from './i18n-capture-png.js'

/**
 * Margins, in device pixels, that macOS leaves around a FOCUSED window's rect when
 * `screencapture -l` writes it onto transparency. The marketing masters are built on
 * these: `app-main` is a 2284x1410 window on a 2508x1634 canvas at +112+76, and the
 * website hero's frame layer is mostly this shadow gradient.
 *
 * Re-derive them by shooting a focused window and reading `magick shot.png
 * -alpha extract -threshold 99% -format '%@' info:`.
 */
export const FOCUSED_SHADOW_X = 112
export const FOCUSED_SHADOW_Y = 76

/**
 * The same margins for an UNFOCUSED window. Not a target, a TRIPWIRE: a window that
 * lost the front position still photographs perfectly, just with a thinner shadow,
 * and the only visible symptom downstream is a hero that lost half its glow. Naming
 * the numbers lets `verifyShadowFrame` say "not focused" instead of "unexpected
 * margin", which is the difference between a fix and a hunt.
 */
export const UNFOCUSED_SHADOW_X = 68
export const UNFOCUSED_SHADOW_Y = 52

/**
 * Alpha at or above which a pixel counts as part of the window rather than its
 * shadow. Matches the `-threshold 99%` that `apps/website/scripts/regenerate-hero.sh`
 * uses on the same images, so the pipeline and the hero compositing agree on where
 * the window ends.
 */
const OPAQUE_ALPHA = 252

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

/**
 * The smallest rect covering every near-opaque pixel of `bytes`, or null when the
 * image has none.
 *
 * ❗ The threshold is the point. A bounding box over "any alpha above zero" would
 * swallow the shadow halo and report the whole canvas, which would make every frame
 * check below pass on exactly the images they exist to reject.
 */
export function opaqueBoundingBox(bytes: Buffer, minAlpha: number = OPAQUE_ALPHA): Rect | null {
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
  if (maxX < 0) return null
  return { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 }
}

export type FrameVerdict = { ok: true; rect: Rect } | { ok: false; reason: string }

/**
 * Whether `bytes` is a usable marketing master: one window of exactly the staged
 * size, sitting on the focused shadow margins.
 *
 * This is the gate the plugin's own native capture cannot provide, because that one
 * returns the bare window rect with no shadow at all. Everything downstream (the
 * hero's frame layer, the alpha-derived window offset in `regenerate-hero.sh`) is
 * built on the shadow being real and focused, so a wrong one fails here rather than
 * shipping.
 */
export function verifyShadowFrame(bytes: Buffer, window: WindowSize): FrameVerdict {
  const rect = opaqueBoundingBox(bytes)
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
        `the photographed window is ${String(rect.width)}x${String(rect.height)}, but the shot was staged at ` +
        `${String(window.width)}x${String(window.height)}. Something resized the window between staging and the shutter.`,
    }
  }
  if (rect.x === UNFOCUSED_SHADOW_X && rect.y === UNFOCUSED_SHADOW_Y) {
    return {
      ok: false,
      reason:
        `the window was not focused when it was shot: its shadow measures ${String(UNFOCUSED_SHADOW_X)}/` +
        `${String(UNFOCUSED_SHADOW_Y)} instead of ${String(FOCUSED_SHADOW_X)}/${String(FOCUSED_SHADOW_Y)}. ` +
        'macOS draws the wide shadow only for the key window, and the hero needs it.',
    }
  }
  if (rect.x !== FOCUSED_SHADOW_X || rect.y !== FOCUSED_SHADOW_Y) {
    return {
      ok: false,
      reason:
        `the window sits at margin +${String(rect.x)}+${String(rect.y)}, not the focused ` +
        `+${String(FOCUSED_SHADOW_X)}+${String(FOCUSED_SHADOW_Y)}. A partly off-screen window crops its own shadow, ` +
        'so move it fully onto one display and re-run.',
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
  width: number
  height: number
  layer: number
  title: string
}

/**
 * The CGWindowID to hand `screencapture -l`, or null when the app has no ordinary
 * window matching.
 *
 * Layer 0 is the only layer an app's real windows live on; menus, popovers, and the
 * menu bar sit above and are frequently LARGER than the window we want, so filtering
 * by layer has to come before picking by area. `titleMatch` is for the child windows
 * (settings), where "largest" would pick the main window instead.
 */
export function pickWindowId(windows: NativeWindow[], titleMatch?: string): number | null {
  const ordinary = windows.filter((candidate) => candidate.layer === 0)
  const matching =
    titleMatch === undefined ? ordinary : ordinary.filter((candidate) => candidate.title.includes(titleMatch))
  if (matching.length === 0) return null
  const largest = matching.reduce((best, candidate) =>
    candidate.width * candidate.height > best.width * best.height ? candidate : best,
  )
  return largest.id
}
