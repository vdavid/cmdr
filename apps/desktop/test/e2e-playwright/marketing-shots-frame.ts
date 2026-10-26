/**
 * Frame arithmetic for the marketing capture: reading a window's rect back out of a
 * `screencapture -l -o` PNG, judging whether it is the bare window at the staged size,
 * restoring the outline `-o` drops, and picking the CGWindowID to shoot.
 *
 * Kept apart from the capture itself so every judgement call in here is a pure
 * function with unit tests. The impure half (`osascript`, `screencapture`) lives in
 * `marketing-shots-native.ts` and is proven by real runs.
 */

import { decodePng } from './i18n-capture-png.js'

/**
 * Width, in device pixels, of the outline the masters carry around the window
 * (`windowOutlineArgs`). A master is the window plus this on every side.
 */
export const WINDOW_OUTLINE_PX = 1

/**
 * Alpha at or above which a pixel counts as part of the window rather than its outline
 * or corner antialiasing: ImageMagick's `-threshold 99%` in 8-bit terms,
 * `ceil(0.99 * 255)`.
 *
 * ❗ The exact value is load-bearing, not a round number. The hero compositing reads
 * the window rect out of the masters with `-alpha extract -threshold 99%`, so a looser
 * threshold here would have this pipeline and `regenerate-hero.sh` disagree about where
 * the window ends. On a master it gives the window at `+1+1`: the outline around it
 * peaks at 0.93 alpha (dark), below the cut.
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
 * Whether `bytes` is a usable SHADOWLESS master (`screencapture -o`): the window alone,
 * filling a canvas of exactly the staged size, transparent only in its rounded corners.
 *
 * A canvas bigger than the window means the shadow came along, which is the one shape
 * this must never accept, since every consumer now adds its own shadow (or none).
 */
export function verifyBareFrame(bytes: Buffer, window: WindowSize): FrameVerdict {
  const { canvas, rect } = frame(bytes, OPAQUE_ALPHA)
  if (rect === null) {
    return {
      ok: false,
      reason:
        'the capture has nothing opaque in it, so no window was photographed. ' +
        'Quit or hide whatever app is frontmost, leave the machine alone, and re-run.',
    }
  }
  if (canvas.width > window.width || canvas.height > window.height) {
    return {
      ok: false,
      reason:
        `the canvas is ${String(canvas.width)}x${String(canvas.height)} around a ${String(rect.width)}x` +
        `${String(rect.height)} window, so the shadow came along. The shutter must pass \`-o\`.`,
    }
  }
  if (rect.x !== 0 || rect.y !== 0 || rect.width !== window.width || rect.height !== window.height) {
    return {
      ok: false,
      reason:
        `the photographed window is ${String(rect.width)}x${String(rect.height)}, but the app reports ` +
        `${String(window.width)}x${String(window.height)}. Something resized the window between staging and the shutter.`,
    }
  }
  return { ok: true, rect }
}

/**
 * Opacity of the 1 px black outline macOS draws just outside a window, per theme.
 *
 * `screencapture -o` drops it along with the shadow (the compositor draws both), and
 * without it a light window melts into a white page. Measured off the shadowed masters
 * by taking the shadow's share back out of the outline pixel, `(t - s) / (1 - s)`, on
 * the left and top edges of both themes, which agreed to within 0.002 (2026-10-26,
 * macOS 27). Dark mode's outline is near-solid; light mode's is a faint grey.
 */
export const WINDOW_OUTLINE_ALPHA = { dark: 0.93, light: 0.21 } as const

export type ShotTheme = keyof typeof WINDOW_OUTLINE_ALPHA

/**
 * Opacity of the white line macOS draws just INSIDE a window's edge, per theme, and
 * its width in device pixels. `-o` drops it with the outline. In dark mode it's what
 * lifts the window off a dark page (the black outline can't), measured on the old
 * shadowed masters as 30 → 53 on the side edge and 42 → 63 on the title bar, both 0.10
 * over 2 px (2026-10-26, macOS 27). Light mode has none worth drawing (245 → 246).
 */
export const WINDOW_INNER_LINE_ALPHA = { dark: 0.1, light: 0 } as const
const WINDOW_INNER_LINE_PX = 2

/**
 * ImageMagick operators that turn a bare `-o` capture (the current image) into one
 * with macOS's window edges restored: the light inner line (dark mode), then the
 * outline. Each traces the window's own alpha (eroded for the inner line, dilated for
 * the outline), which is what makes both follow the rounded corners exactly,
 * antialiasing included.
 */
export function windowOutlineArgs(theme: ShotTheme): string[] {
  const inner = WINDOW_INNER_LINE_ALPHA[theme]
  return [
    // Pad FIRST: the capture is the window edge to edge, and morphology reads beyond the
    // canvas as copies of its edge, so an unpadded erosion finds no edge to erode.
    '-bordercolor',
    'none',
    '-border',
    String(WINDOW_OUTLINE_PX),
    ...(inner > 0 ? innerLineArgs(inner) : []),
    ...outlineArgs(theme),
  ]
}

/** The band the window's alpha loses when eroded, painted white at `alpha` on top of the window. */
function innerLineArgs(alpha: number): string[] {
  return [
    '(',
    '+clone',
    '-alpha',
    'extract',
    '(',
    '+clone',
    '-morphology',
    'Erode',
    `Disk:${String(WINDOW_INNER_LINE_PX)}`,
    ')',
    '-compose',
    'minus_src',
    '-composite',
    '-evaluate',
    'multiply',
    String(alpha),
    ')',
    '(',
    '+clone',
    '-fill',
    'white',
    '-colorize',
    '100',
    '-clone',
    '1',
    '-alpha',
    'off',
    '-compose',
    'CopyOpacity',
    '-composite',
    ')',
    '-delete',
    '1',
    // `src-atop` keeps the window's own alpha, so the corners' antialiasing survives.
    '-compose',
    'src-atop',
    '-composite',
  ]
}

/**
 * Takes the ring the window's alpha gains when dilated by 1 px (into the padding),
 * paints it black at the theme's opacity, and lays the window back on top.
 */
function outlineArgs(theme: ShotTheme): string[] {
  return [
    '(',
    '+clone',
    '-alpha',
    'extract',
    ')',
    '(',
    '+clone',
    '-morphology',
    'Dilate',
    'Disk:1',
    ')',
    '(',
    '-clone',
    '2',
    '-clone',
    '1',
    '-compose',
    'minus_src',
    '-composite',
    '-evaluate',
    'multiply',
    String(WINDOW_OUTLINE_ALPHA[theme]),
    ')',
    '(',
    '-clone',
    '3',
    '-fill',
    'black',
    '-colorize',
    '100',
    '-clone',
    '3',
    '-alpha',
    'off',
    '-compose',
    'CopyOpacity',
    '-composite',
    ')',
    '-delete',
    '1-3',
    '+swap',
    '-compose',
    'over',
    '-composite',
  ]
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
