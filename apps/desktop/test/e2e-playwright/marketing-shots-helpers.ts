/**
 * The marketing capture's shutter, and the window bookkeeping around it.
 *
 * Mirrors `shoot()` in `i18n-capture-helpers.ts` — settle, shoot, verify the PIXELS,
 * retry, fail loudly — and differs in exactly one way: it photographs through
 * `screencapture -l` rather than the Playwright plugin, because the plugin's native
 * capture returns the bare window rect with no macOS shadow, and every marketing
 * master is built on that shadow.
 *
 * ❌ Never swap in `page.screenshot()` "just for one shot": a run once wrote 31 blank
 * images with a fully correct DOM and every other gate green. Only the image bytes
 * catch that.
 */

import { spawnSync } from 'node:child_process'
import { readFileSync, renameSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { assessImageContent } from './i18n-capture-png.js'
import { verifyShadowFrame } from './marketing-shots-frame.js'
import type { Rect, WindowSize } from './marketing-shots-frame.js'
import { captureWindow, focusApp, requireWindowId } from './marketing-shots-native.js'

/** The `evaluate` surface every scoped Playwright page gives us. */
export interface EvaluatablePage {
  evaluate: {
    (js: string): Promise<unknown>
    <T>(js: string): Promise<T>
  }
}

/** Attempts a shot gets before the master is failed outright, matching `shoot()`. */
const SHOT_ATTEMPTS = 3

/** Where masters land. The orchestrator picks it so `--out` can keep `brand/` clean. */
export function outputDir(): string {
  const dir = process.env.CMDR_SHOTS_OUT_DIR
  if (dir === undefined || dir === '') {
    throw new Error('CMDR_SHOTS_OUT_DIR is unset. Run this shard through `pnpm marketing:shots`, not bare Playwright.')
  }
  return dir
}

/** The app's pid, handed over by the orchestrator: nothing exposes it over the socket. */
export function appPid(): number {
  const pid = Number(process.env.CMDR_SHOTS_PID)
  if (!Number.isInteger(pid) || pid <= 0) {
    throw new Error('CMDR_SHOTS_PID is unset or not a pid. Run this shard through `pnpm marketing:shots`.')
  }
  return pid
}

/** A window's logical size and the display scale it is rendered at. */
export interface WindowMetrics {
  /** Points, which is what `CGWindowBounds` reports and what window matching uses. */
  logical: WindowSize
  /** Device pixels, which is what the captured PNG measures in. */
  device: WindowSize
  scale: number
}

/**
 * Reads a window's live size and scale off the app.
 *
 * ❗ Read, never assume. The settings window's size is derived from the system text
 * scale, so a constant would be right only on the machine it was measured on; and every
 * margin this pipeline gates on is a DEVICE-pixel number, so a 1x display has to fail
 * with a message that says so rather than with arithmetic that looks broken.
 */
export async function windowMetrics(page: EvaluatablePage, label: string): Promise<WindowMetrics> {
  const labelJson = JSON.stringify(label)
  const raw = await page.evaluate<{ width: number; height: number; scale: number }>(
    `(async () => {
       const invoke = window.__TAURI_INTERNALS__.invoke
       const size = await invoke('plugin:window|inner_size', { label: ${labelJson} })
       const scale = await invoke('plugin:window|scale_factor', { label: ${labelJson} })
       return { width: size.width, height: size.height, scale }
     })()`,
  )
  // `inner_size` is already in physical pixels; the logical size is what the window
  // server lists, so both are derived from one read rather than two racing ones.
  const device = { width: raw.width, height: raw.height }
  const logical = { width: Math.round(raw.width / raw.scale), height: Math.round(raw.height / raw.scale) }
  return { logical, device, scale: raw.scale }
}

/** Resizes a window to an exact LOGICAL size, the only thing the hero geometry depends on. */
export async function setWindowSize(
  page: EvaluatablePage,
  label: string,
  width: number,
  height: number,
): Promise<void> {
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('plugin:window|set_size', {
       label: ${JSON.stringify(label)},
       value: { Logical: { width: ${String(width)}, height: ${String(height)} } }
     })`,
  )
}

/**
 * Moves an accepted shot into place, converting to WebP when that's what was asked for.
 *
 * ❗ The shutter and every pixel gate stay on the PNG `screencapture` writes; only the
 * FILE that lands in `brand/` is WebP. Lossless, so the masters are pixel-identical to
 * the PNG (verified: `magick compare -metric AE` reads 0) at about a fifth of the bytes,
 * which is what keeps a reshoot from adding ~8 MB of undeltifiable blobs to git.
 *
 * ❌ Never lossy here. These are the pristine originals every other surface is cut from.
 */
function writeMaster(stagePath: string, path: string): void {
  if (!path.endsWith('.webp')) {
    renameSync(stagePath, path)
    return
  }
  const res = spawnSync('magick', [stagePath, '-define', 'webp:lossless=true', '-define', 'webp:method=6', path], {
    encoding: 'utf8',
  })
  if (res.error !== undefined || res.status !== 0) {
    throw new Error(
      `Converting the shot to WebP failed (\`magick\` from ImageMagick). ${res.stderr ?? String(res.error)}`,
    )
  }
}

/** Thrown when the shutter cannot produce a usable master, carrying every attempt's reason. */
export class ShotRejectedError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'ShotRejectedError'
  }
}

/**
 * Takes ONE verified master of `label`'s window into `<out>/<filename>`, and returns
 * the window rect inside the canvas.
 *
 * Per attempt: bring the app to the front from OUTSIDE (macOS composites and shadows
 * only the key window, and the app cannot claim that itself as a raw binary), let the
 * paint settle, shoot, then judge the BYTES twice — once for content, once for the
 * frame. Three attempts, then a failure that names the cause up front.
 */
export async function shootWithShadow(
  page: EvaluatablePage,
  label: string,
  filename: string,
  metrics: WindowMetrics,
): Promise<Rect> {
  const path = join(outputDir(), filename)
  const pid = appPid()
  const reasons: string[] = []

  for (let attempt = 1; attempt <= SHOT_ATTEMPTS; attempt++) {
    // Each attempt shoots to its OWN staging file and only the winner is renamed into
    // place, so a rejected attempt can never leave a bad image where a good one was.
    const stagePath = `${path}.staged-${String(attempt)}`
    try {
      focusApp(pid)
      await settlePaint(page)
      const windowId = requireWindowId(pid, metrics.logical, label)
      captureWindow(windowId, stagePath)

      const bytes = readFileSync(stagePath)
      const content = assessImageContent(bytes)
      if (!content.ok) {
        reasons.push(`attempt ${String(attempt)}: ${content.reason}`)
        continue
      }
      const framing = verifyShadowFrame(bytes, metrics.device)
      if (!framing.ok) {
        reasons.push(`attempt ${String(attempt)}: ${framing.reason}`)
        continue
      }

      writeMaster(stagePath, path)
      if (attempt > 1) console.log(`[marketing-shots] ${filename}: usable on attempt ${String(attempt)}`)
      return framing.rect
    } finally {
      rmSync(stagePath, { force: true })
    }
  }

  throw new ShotRejectedError(
    `Could not get a usable master for \`${filename}\` in ${String(SHOT_ATTEMPTS)} tries. ` +
      'Quit or hide whatever app is frontmost, leave the machine alone, and re-run: macOS composites and ' +
      `shadows only the key window. (${reasons.join('; ')})`,
  )
}

/**
 * Waits for the webview to have PAINTED, not for a duration.
 *
 * Same shape as `settlePaint` in `i18n-capture-helpers.ts`, inlined rather than
 * imported because that module resolves the translator screenshot directory at import
 * time and would drag the whole i18n capture config into this shard. Resolves on the
 * second animation frame, racing a short timeout because `requestAnimationFrame` is
 * throttled on a window that isn't foreground and would otherwise hang the eval. The
 * timeout is a safety net; the pixel checks above are what actually guard the result.
 */
async function settlePaint(page: EvaluatablePage): Promise<void> {
  await page.evaluate(`new Promise(function(resolve) {
    var done = false;
    var finish = function() { if (!done) { done = true; resolve(true); } };
    requestAnimationFrame(function() { requestAnimationFrame(finish); });
    setTimeout(finish, 500);
  })`)
}
