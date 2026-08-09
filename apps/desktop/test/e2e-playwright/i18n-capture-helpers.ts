/**
 * Shared capture machinery for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * Holds the reusable primitives every surface group leans on: the capture-sink
 * RPC (`captureCall` / `keysFor`), the paint/focus settling helpers, the shutter
 * (`shoot`), the shared types, and the two surface-capturing engines
 * (`captureSurface` for reactive mounted markup, `captureToastSurface` for
 * snapshot-resolved toasts). The per-group capture functions live in
 * `i18n-capture-surfaces.ts` and the orchestration in the spec; both import from
 * here.
 *
 * Its neighbors, split out for the file-length budget: `i18n-capture-config.ts`
 * (which pass is running, where artifacts go), `i18n-capture-frame.ts` (window
 * framing, fitting, toast hygiene, the clip scan), and `i18n-capture-png.ts`
 * (decode, encode, crop, the blank check).
 */

import { readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect } from './fixtures.js'
import { dismissAllToasts, getFixtureRoot } from './helpers.js'
import { assessImageContent, cropPng, isCompletePng } from './i18n-capture-png.js'
import {
  DEFAULT_UI_ZOOM,
  MAX_UI_ZOOM,
  isOverflowPass,
  isWorstCasePass,
  overflowLocale,
  screenshotsDir,
} from './i18n-capture-config.js'
import {
  clearStrayToasts,
  fitWindowToContent,
  measureCropGeometry,
  readToastSignature,
  scanForClipping,
  selectorList,
  straysIn,
  type FitOutcome,
  type FrameSelector,
  type WindowFit,
} from './i18n-capture-frame.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

interface CaptureApi {
  enable: () => boolean
  disable: () => void
  setSurface: (label: string) => void
  dump: () => Record<string, string[]>
  reset: () => void
  rerender: () => void
  setLocale: (tag: string | null) => void
  setTextSize: (percent: number) => Promise<void>
}

/** Calls a method on the webview's `window.__cmdrI18nCapture`, returns its result. */
export async function captureCall<T>(page: TauriPage, method: keyof CaptureApi, arg?: string): Promise<T> {
  const argJson = arg === undefined ? '' : JSON.stringify(arg)
  return page.evaluate<T>(`(function() {
    var api = window.__cmdrI18nCapture;
    if (!api) throw new Error('__cmdrI18nCapture not installed; build with playwright-e2e and ensure non-prod mode');
    return api.${method}(${argJson});
  })()`)
}

/** Catalog keys recorded for `surface`, sorted, read from the live sink. */
export async function keysFor(page: TauriPage, surface: string): Promise<string[]> {
  const dump = await captureCall<Record<string, string[]>>(page, 'dump')
  return dump[surface] ?? []
}

/**
 * Waits for the webview to composite a fresh frame before a native screenshot.
 * The native (CoreGraphics) capture grabs the window's last COMPOSITED frame,
 * which lags a just-applied DOM change (a freshly-opened modal), so without this
 * the modal can be missing from the image.
 *
 * Resolves on the next animation frame, BUT races a short timeout:
 * `requestAnimationFrame` is throttled/paused on a window that isn't foreground,
 * where it would otherwise never fire and hang the eval. The timeout is a safety
 * net, not the primary signal: a foreground window resolves on the real frame in
 * ~16 ms, and `shoot()`'s pixel check is what actually guards the result.
 */
export async function settlePaint(page: TauriPage): Promise<void> {
  await page.evaluate(`new Promise(function(resolve) {
    var done = false;
    var finish = function() { if (!done) { done = true; resolve(true); } };
    requestAnimationFrame(function() { requestAnimationFrame(finish); });
    setTimeout(finish, 500);
  })`)
}

/**
 * Brings a window frontmost via `plugin:window|set_focus`. Needed both to unstall
 * a window's occluded-throttled async `onMount` (settings/shortcuts gate content
 * on it) and so macOS composites the current frame for the native screenshot.
 * `core:window:allow-set-focus` is granted in each window's capability.
 */
export async function focusWindow(page: TauriPage, label: string): Promise<void> {
  const labelJson = JSON.stringify(label)
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_focus', { label: ${labelJson} })`)
}

/**
 * Attempts a surface's shot gets before the surface is failed outright.
 * `SHOT_ATTEMPTS_WORD` is the same number spelled out for the failure copy (house
 * style spells one through nine); keep the two in sync.
 */
const SHOT_ATTEMPTS = 3
const SHOT_ATTEMPTS_WORD = 'three'

/**
 * How long to wait for a requested screenshot to actually become a whole file on
 * disk, and how often to look.
 *
 * The plugin's `native_screenshot` command returns BEFORE its PNG write lands, so
 * reading the path the instant the call resolves finds a missing or half-written
 * file. Under a burst of shots (the 13 indexing tiles) the writer falls seconds
 * behind. Waiting on the file's own completeness marker is the honest fix, and
 * the ceiling is generous because a slow write is normal here while a never-
 * arriving one is a real failure worth reporting.
 */
const SHOT_FILE_TIMEOUT_MS = 20000
const SHOT_FILE_POLL_MS = 25

/**
 * Screenshots `page`'s window into `path` and returns the complete PNG bytes, or
 * null if a whole file never showed up in time. Waits for `isCompletePng` (the
 * file's own IEND terminator) rather than trusting the command's resolution.
 */
async function screenshotToFile(page: TauriPage, path: string): Promise<Buffer | null> {
  await page.screenshot({ path })
  const deadline = Date.now() + SHOT_FILE_TIMEOUT_MS
  for (;;) {
    const bytes = readIfComplete(path)
    if (bytes !== null) return bytes
    if (Date.now() >= deadline) return null
    await new Promise((resolve) => setTimeout(resolve, SHOT_FILE_POLL_MS))
  }
}

/** Reads `path` if it's there AND whole; null while it's missing or partial. */
function readIfComplete(path: string): Buffer | null {
  try {
    const bytes = readFileSync(path)
    return isCompletePng(bytes) ? bytes : null
  } catch {
    return null // not written yet
  }
}

/**
 * Thrown when `shoot` cannot get real content into the PNG. Its own type so a
 * caller that treats a staging failure as a documented skip (the dialog gallery)
 * can still treat an unphotographable surface as a real failure.
 */
export class BlankShotError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'BlankShotError'
  }
}

/**
 * What a reader (agent or human) needs the moment a capture comes back blank:
 * the FIX first, then why. The cause is almost never the app or the harness —
 * macOS stops compositing a window that isn't frontmost, so anyone clicking into
 * another app mid-run leaves the capture reading a stale, pre-paint frame — so
 * say that plainly and stop someone hunting a bug in Cmdr that isn't there.
 *
 * DRAFT (David reviews human-facing copy).
 */
const BLANK_SHOT_EXPLANATION =
  'Quit or hide whatever app is frontmost, leave the machine alone, and re-run. macOS stops compositing a ' +
  "window that isn't frontmost, so the capture reads a stale frame from before the UI painted. An idle " +
  'machine is not enough on its own: this binary runs outside LaunchServices, so it cannot take the front ' +
  'position from an app that already holds it, and a run can go blank with nobody touching the laptop. ' +
  'Either way the cause is the front position, not a broken Cmdr or harness.'

/** Per-shot framing and hygiene knobs. Every field is optional; the defaults suit a plain window shot. */
export interface ShotOptions {
  /**
   * Crop the written PNG to this element's bounds (plus padding) instead of
   * keeping the whole window. Used for the surfaces where the window frame is
   * noise rather than context: the registry-driven soft dialogs, toasts, and the
   * per-tile indexing review images. Everything else keeps its window on purpose.
   */
  cropSelector?: FrameSelector
  /**
   * Padding around `cropSelector`, in CSS px. Defaults to the roomy dialog value;
   * pass `CROP_PADDING_TIGHT_CSS_PX` for an element packed beside its neighbors,
   * where roomy padding just frames the neighbors too.
   */
  cropPadding?: number
  /**
   * Toast texts this surface deliberately staged. Anything else in the toast
   * layer is a stray: dismissed before the shot, and grounds for a re-shoot if it
   * turns up during one. Defaults to none, which is right for nearly every
   * surface.
   */
  expectedToasts?: string[]
}

/**
 * Crops the already-verified `bytes` to `cropSelector`, or returns null to keep
 * the full window (nothing matched, the rect was degenerate, or this is an
 * overflow pass, where the window's own edges are the point of the image).
 *
 * ❗ The rect comes from `getBoundingClientRect() * devicePixelRatio`, which maps
 * 1:1 onto image pixels only while the webview covers the whole window with no
 * chrome offset. That's true because Cmdr draws its own title bar inside the
 * webview (the traffic lights sit on the `.title-bar` element) — and it's checked
 * here rather than assumed, because a silently-shifted crop would frame the wrong
 * thing in every affected screenshot at once.
 */
async function cropIfRequested(
  page: TauriPage,
  bytes: Buffer,
  imageWidth: number,
  imageHeight: number,
  cropSelector: FrameSelector | undefined,
  cropPadding: number | undefined,
): Promise<Buffer | null> {
  if (cropSelector === undefined) return null
  // The overflow passes exist to show text colliding with the window's edges, so
  // cropping away the edges would defeat them.
  if (isOverflowPass) return null
  const geometry = await measureCropGeometry(page, cropSelector, cropPadding)
  if (geometry === null) return null
  if (geometry.expectedImageWidth !== imageWidth || geometry.expectedImageHeight !== imageHeight) {
    throw new Error(
      `Cannot crop to \`${selectorList(cropSelector).join(' / ')}\`: the window's image is ${String(imageWidth)}x${String(imageHeight)} but ` +
        `the webview measures ${String(geometry.expectedImageWidth)}x${String(geometry.expectedImageHeight)}. ` +
        'Layout coordinates only map onto image pixels while the webview covers the whole window; ' +
        'something now draws outside it, so every crop rect would be offset. Fix the mapping or stop cropping.',
    )
  }
  return cropPng(bytes, geometry.rect)
}

/**
 * Takes ONE verified native screenshot of `page`'s window into `screenshot`.
 *
 * The contract every caller relies on: when this returns, a real image of the
 * CURRENT UI is on disk. When it can't get one, it throws, and the surface lands
 * in `capture-failed.json` and fails the run.
 *
 * Per attempt: bring the window to the front (`set_focus`), clear any toast the
 * surface didn't stage, settle the paint, shoot, wait for the WHOLE PNG to land on
 * disk (the capture's write outlives its command), then decode it and check it
 * carries content AND that no unstaged toast arrived while the shutter was open.
 * The pixels are the only real guard — the focus and settle steps are the remedy,
 * not the proof.
 *
 * ❌ Never replace the pixel check with "wait N seconds and hope". A run once
 * wrote 31 blank images with the DOM fully correct, every gate passed, and a green
 * result; only the image bytes could have caught it.
 */
export async function shoot(
  page: TauriPage,
  windowLabel: string,
  screenshot: string,
  options: ShotOptions = {},
): Promise<void> {
  const path = join(screenshotsDir, screenshot)
  const tries: string[] = []
  // Each attempt shoots to its OWN staging file and only the winner is renamed
  // into place. Reusing one path would let a slow write from a rejected attempt
  // land on top of a good image seconds later.
  const staging: string[] = []
  try {
    for (let attempt = 1; attempt <= SHOT_ATTEMPTS; attempt++) {
      const stagePath = `${path}.staged-${String(attempt)}`
      staging.push(stagePath)
      // Order the window to the front so macOS composites it. This is the remedy a
      // retry leans on: whatever stole the front position (usually someone using
      // the computer), asking for it back is what can fix the next attempt.
      await focusWindow(page, windowLabel).catch(() => {})
      const result = await attemptShot(page, stagePath, options)
      if (result.bytes !== null) {
        if (result.cropped === null) renameSync(stagePath, path)
        else writeFileSync(path, result.cropped)
        if (attempt > 1) console.log(`[i18n-capture] ${screenshot}: real content on attempt ${String(attempt)}`)
        return
      }
      tries.push(`attempt ${String(attempt)}: ${result.problem}`)
      console.warn(
        `[i18n-capture] ${screenshot}: ${result.problem}; re-focusing window '${windowLabel}' and re-shooting`,
      )
    }
  } finally {
    for (const stagePath of staging) rmSync(stagePath, { force: true })
  }
  throw new BlankShotError(
    `Captured a blank frame for \`${screenshot}\` after ${SHOT_ATTEMPTS_WORD} tries. ` +
      `${BLANK_SHOT_EXPLANATION} (${tries.join('; ')})`,
  )
}

/** One attempt's verdict: the accepted bytes (plus any crop), or the reason to re-shoot. */
interface ShotAttempt {
  /** The verified full-window PNG, or null when this attempt didn't earn one. */
  bytes: Buffer | null
  /** The cropped replacement to write, or null to keep the staged full-window file. */
  cropped: Buffer | null
  /** Why this attempt failed; empty when it succeeded. */
  problem: string
}

/**
 * Runs ONE attempt: clear strays, settle, shoot, verify the pixels, verify no
 * toast gate-crashed the frame, and frame the result. Split out of `shoot` so the
 * retry loop stays about retrying and each gate reads as its own step.
 */
async function attemptShot(page: TauriPage, stagePath: string, options: ShotOptions): Promise<ShotAttempt> {
  const { cropSelector, cropPadding, expectedToasts = [] } = options
  const miss = (problem: string): ShotAttempt => ({ bytes: null, cropped: null, problem })

  // Precondition: the toast layer holds exactly what this surface staged. A toast
  // nothing here asked for (the virtual MTP device announcing itself) would
  // otherwise be photographed over the surface.
  try {
    await clearStrayToasts(page, expectedToasts)
  } catch (err) {
    return miss(`a stray toast would not dismiss: ${err instanceof Error ? err.message : String(err)}`)
  }

  await settlePaint(page)
  const written = await screenshotToFile(page, stagePath)
  if (written === null) {
    return miss(`the capture never finished writing a PNG (waited ${String(SHOT_FILE_TIMEOUT_MS)} ms)`)
  }
  const verdict = assessImageContent(written)
  if (!verdict.ok) return miss(verdict.reason)

  // Postcondition, and the half that actually makes this airtight: a toast can
  // appear BETWEEN the precondition and the shutter, which is exactly how a
  // half-faded toast ended up over unrelated dialogs.
  const live = await readToastSignature(page)
  const late = straysIn(live, expectedToasts)
  if (late.length > 0) return miss(`a toast nothing staged appeared mid-shot (${late.join('; ')})`)
  // The mirror image, and the reason it's checked here rather than trusted: a
  // TRANSIENT toast auto-dismisses on its own clock, so a toast surface can lose
  // the very thing it exists to photograph between staging and the shutter. The
  // crop would then frame an empty layer and quietly fall back to a picture of
  // the file panes under a `toast-…` filename.
  if (expectedToasts.length > 0 && live.length === 0) {
    return miss('the staged toast was gone before the shutter (it auto-dismissed)')
  }

  const cropped = await cropIfRequested(page, written, verdict.width, verdict.height, cropSelector, cropPadding)
  return { bytes: written, cropped, problem: '' }
}

/**
 * Reads the live effective UI scale off the `--font-scale` root var that
 * `text-size.svelte`'s `computeAndApply` writes. The worst-case staging polls
 * this after `setTextSize` so it resizes the window only once the new scale (and
 * the settings window's live min-size recompute) has applied.
 */
async function readFontScale(page: TauriPage): Promise<number> {
  return page.evaluate<number>(
    `parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--font-scale')) || 1`,
  )
}

/**
 * The minimum allowed LOGICAL size of each window at the worst-case zoom
 * (`MAX_UI_ZOOM`). Tauri does NOT clamp a programmatic `setSize` to the window's
 * `minWidth`/`minHeight` (requesting 1x1 actually shrinks to ~1px), so we set the
 * EXACT minimum rather than a tiny value. Values mirror the window creators (keep
 * in sync): `tauri.conf.json` (main, fixed 950x550), `settings-window.ts`
 * (`SETTINGS_CHROME_WIDTH + SETTINGS_CONTENT_BASE_MIN_WIDTH*scale` by
 * `SETTINGS_BASE_MIN_HEIGHT*scale`), `open-viewer.ts` (viewer, fixed 400x300),
 * `shortcuts-window.ts` (`MIN_WIDTH*scale` by `MIN_HEIGHT`), and `queue-window.ts`
 * (`MIN_WIDTH*scale` by `MIN_HEIGHT*scale`, both scaled). The scaled ones use the
 * worst-case scale (`MAX_UI_ZOOM/100`).
 */
function minSizeFor(label: string): { width: number; height: number } {
  const scale = MAX_UI_ZOOM / 100
  if (label === 'settings') return { width: 252 + 348 * scale, height: 400 * scale }
  if (label === 'shortcuts') return { width: 300 * scale, height: 420 }
  if (label === 'queue') return { width: 540 * scale, height: 280 * scale }
  if (label.startsWith('viewer')) return { width: 400, height: 300 }
  // main window (and any main-window overlay captured against `main`).
  return { width: 950, height: 550 }
}

/**
 * Resizes a window to its minimum allowed size for the worst-case pass. Invokes
 * the window plugin directly with the IPC payload shape `setSize` produces
 * (`{ label, value: { Logical: { width, height } } }`); the
 * `core:window:allow-set-size` permission is granted only in the E2E capture
 * build (the build.rs-generated `playwright.json`), never in production. Tauri
 * doesn't clamp `setSize` to the min constraint, so we pass the exact minimum
 * (see `minSizeFor`).
 */
async function resizeWindowToMin(page: TauriPage, label: string): Promise<void> {
  const labelJson = JSON.stringify(label)
  const { width, height } = minSizeFor(label)
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_size', {
    label: ${labelJson},
    value: { Logical: { width: ${String(width)}, height: ${String(height)} } }
  })`)
}

/**
 * Stages the WORST-CASE layout stress on `page`'s window before its shot + clip
 * scan: maxes the UI zoom (`MAX_UI_ZOOM`) via the production `setSetting` path,
 * waits for the new scale to apply, then shrinks the window to its minimum so the
 * inflated pseudolocale text fights the tightest box the app permits. No-op
 * outside the worst-case pass, so the default overflow pass is untouched.
 *
 * `label` is the window to resize (`'main'` for main-window overlays, the
 * separate window's label otherwise). Best-effort per step: a window that
 * refuses to shrink below its content (a documented case to note, not fake)
 * leaves the resize logged rather than aborting the surface.
 */
export async function stressLayoutIfWorstCase(page: TauriPage, label: string): Promise<void> {
  if (!isWorstCasePass) return
  // Drive the real zoom path; cross-window-synced + re-runs computeAndApply.
  // The QUEUE window can't drive it: `capabilities/queue.json` deliberately drops
  // `store:default`, so its `setSetting` write is ACL-denied. Harmless, because
  // the zoom is cross-window-synced and every main-window surface ran first, so
  // the app is already at max zoom by the time the queue window opens. The resize
  // below is what this window actually needs.
  if (label === 'queue') {
    await captureCall(page, 'setTextSize', String(MAX_UI_ZOOM)).catch(() => {})
  } else {
    await captureCall(page, 'setTextSize', String(MAX_UI_ZOOM))
  }
  // Wait for the scale to land (and the settings window's live min-size effect to
  // recompute) before resizing, so the clamp targets the max-zoom floor. Polls
  // the live `--font-scale`; falls through on timeout rather than hanging.
  await expect
    .poll(async () => readFontScale(page), { timeout: 3000 })
    .toBeGreaterThanOrEqual(MAX_UI_ZOOM / 100 - 0.01)
    .catch(() => {})
  try {
    await resizeWindowToMin(page, label)
  } catch (err) {
    console.warn(`[i18n-overflow] worst-case: could not shrink window '${label}' to min: ${String(err)}`)
  }
  // Let the layout reflow at the new (min) size before the shot + clip scan.
  await settlePaint(page)
}

/** A surface's report entry: the screenshot file and the keys recorded for it. */
export interface SurfaceEntry {
  screenshot: string
  keys: string[]
  /**
   * The UI zoom this surface was photographed at, present ONLY when it isn't the
   * default 100%. A surface that couldn't fit on the display even at full window
   * height is captured smaller, and the coverage report says so, so a translator
   * doesn't read that image as "this is how big the text is".
   */
  uiZoom?: number
}

/** What fitting achieved per surface, for the run's report. Empty for surfaces that needed no fitting. */
export const fitFindings: Record<string, FitOutcome> = {}

/**
 * Grows `windowLabel` so `selector`'s content fits before the shot, wiring the
 * shared paint-settle and the production zoom path into the pure fitting loop.
 * No-op (null) in the worst-case overflow pass, which deliberately shrinks every
 * window to its minimum: growing it back would erase exactly what that pass is
 * for.
 *
 * OPT-IN per surface, not automatic for every dialog: some surfaces scroll BY
 * DESIGN (the command palette lists every command), and growing a window until
 * such a list fits would produce a screen-tall window that shows nothing useful.
 * Callers name the surfaces where scrolling means "cut off".
 */
export async function fitSurfaceWindow(
  page: TauriPage,
  windowLabel: string,
  selector: FrameSelector,
): Promise<WindowFit | null> {
  if (isWorstCasePass) return null
  return fitWindowToContent(page, windowLabel, selector, settlePaint, async (target, percent) => {
    await captureCall(target, 'setTextSize', String(percent))
  })
}

/** Records a fit outcome under `label` when it did anything worth reporting. */
export function recordFit(label: string, fit: WindowFit | null): void {
  if (fit === null) return
  const { grewBy, zoom, residual, unreachable } = fit.outcome
  if (grewBy === 0 && zoom === DEFAULT_UI_ZOOM && residual === 0 && unreachable.length === 0) return
  fitFindings[label] = fit.outcome
  if (zoom !== DEFAULT_UI_ZOOM) {
    console.warn(`[i18n-capture] ${label}: captured at ${String(zoom)}% UI zoom (it doesn't fit this display at 100%)`)
  }
  if (unreachable.length > 0) {
    console.warn(`[i18n-capture] ${label}: content clipped with no way to scroll it: ${unreachable.join(', ')}`)
  }
}

/** What a surface's `stage` step hands back to `captureSurface`. */
export interface StagedSurface {
  /** The page whose capture sink + screenshot this surface uses. */
  page: TauriPage
  /**
   * Label of the WINDOW this surface lives in, when it isn't the main window.
   * `shoot` brings it frontmost before the capture. Omit for surfaces hosted on
   * the main window (every overlay, dialog, and toast), which default to `main`.
   */
  focusLabel?: string
  /**
   * A selector that must STILL match immediately before the shot. Pass the same
   * content selector the staging waited on.
   *
   * Staging proves a surface was ready at SOME earlier moment; this proves it's
   * ready in the frame we actually photograph, which is the only moment that
   * matters. The acknowledgements dialog shipped a picture of its "Loading the
   * list…" spinner while its report entry recorded the loaded state's keys,
   * exactly because the two moments were allowed to differ. ❗ Name something the
   * LOADED state renders (a package row), never the container that holds both a
   * spinner and the content.
   */
  readySelector?: string
  /**
   * Grow the window until THIS element's content stops scrolling, so the shot
   * isn't cut off at the bottom. Name the surface's own frame (the
   * `.modal-dialog`, the settings content pane), and only where scrolling means
   * "cut off" rather than "this list is long by design". See `fitSurfaceWindow`.
   */
  fitSelector?: FrameSelector
}

/**
 * Stages, captures, and records ONE surface, isolating its failure: any throw is
 * caught, logged, and pushed to `failed`, and the run continues to the next
 * surface. Without this isolation a single broken surface (e.g. a window that
 * won't load) aborts the whole driver before the report is written, discarding
 * every surface that already succeeded: fatal whack-a-mole for a ~50-surface
 * capture. The test still fails at the end if `failed` is non-empty (see the
 * final `expect`), but only AFTER every surface is attempted and the report
 * written.
 *
 * `stage` does the surface-specific work (open the window, navigate, enable the
 * sink) and returns the page to capture against. `captureSurface` then runs the
 * common tail: `setSurface` → `rerender` (re-resolves every mounted reactive
 * `t()`/`<Trans>` under this surface, recording its keys) → optional `set_focus`
 * → `settlePaint` → native screenshot → read the keys back. The capture sink's
 * enable/reset stays in `stage` because it's per-WINDOW, not per-surface (one
 * window hosts several surfaces sharing one sink).
 */
export async function captureSurface(
  label: string,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  stage: () => Promise<StagedSurface>,
): Promise<void> {
  const screenshot = `${label}.png`
  try {
    const { page, focusLabel, readySelector, fitSelector } = await stage()
    // Overflow pass: each separate WebviewWindow (settings, viewer, shortcuts)
    // has its own locale source, so set the pseudolocale on whatever page this
    // surface captures against. Idempotent on `main` (already switched in the
    // first surface). The `rerender` below then re-resolves it in the expanded
    // strings before the screenshot + clip scan. No-op outside an overflow pass.
    if (isOverflowPass) await captureCall(page, 'setLocale', overflowLocale)
    await captureCall(page, 'setSurface', label)
    await captureCall(page, 'rerender')
    // Worst-case pass: max the zoom and shrink the window to its min before the
    // shot. Resize the window this surface lives in (`focusLabel` for a separate
    // window, else `main`). No-op outside the worst-case pass.
    const windowLabel = focusLabel ?? 'main'
    await stressLayoutIfWorstCase(page, windowLabel)
    // Re-prove readiness in the frame we're about to photograph. Staging ran
    // several steps ago and only proved the surface was ready THEN; anything
    // since (a re-render, an async remount, a state that fell back to a spinner)
    // can have undone it, and the resulting image looks plausible enough to ship.
    // Throwing here fails the surface loudly instead.
    if (readySelector !== undefined) await page.waitForSelector(readySelector, 5000)
    // Grow the window so a surface taller than its frame isn't photographed cut
    // off at the bottom. Restored right after the shot so the next surface starts
    // from the same window as every other run.
    const fit = fitSelector === undefined ? null : await fitSurfaceWindow(page, windowLabel, fitSelector)
    try {
      // `shoot` owns focus, paint settling, and verifying the pixels that landed.
      // The MAIN window needs the focus step as much as a separate window does: it
      // loses key status to every settings/viewer/shortcuts window the run opens,
      // and macOS then hands the capture a stale frame for every main-window surface
      // that follows. That's what produced a run of blank dialogs.
      await shoot(page, windowLabel, screenshot)
      // Scan while the window is still the one that got photographed, so a clip
      // finding always describes the image next to it in the report.
      await scanForClipping(page, label)
    } finally {
      recordFit(label, fit)
      await fit?.restore()
    }
    const zoom = fit?.outcome.zoom ?? DEFAULT_UI_ZOOM
    report[label] = { screenshot, keys: await keysFor(page, label) }
    if (zoom !== DEFAULT_UI_ZOOM) report[label].uiZoom = zoom
    console.log(`[i18n-capture] ${label}: ${String(report[label].keys.length)} keys → ${screenshot}`)
  } catch (err) {
    failed.push(label)
    console.warn(`[i18n-capture] surface ${label} FAILED: ${err instanceof Error ? err.message : String(err)}`)
  }
}

/**
 * Waits for the first toast's enter animation (0.2s slide-in: opacity 0→1,
 * translateX 20→0) to FINISH, so the native screenshot captures a fully-rendered
 * toast rather than a mid-fade frame. Polls the live computed style for a settled
 * opacity (1) and transform (`none` or an identity matrix, no residual X
 * translation). A short deadline keeps a `prefers-reduced-motion` build (no
 * animation, instantly settled) from waiting needlessly.
 */
async function waitForToastSettled(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () =>
        page.evaluate<boolean>(`(function(){
          var toast = document.querySelector('.toast');
          if (!toast) return false;
          var s = getComputedStyle(toast);
          if (s.opacity !== '1') return false;
          var t = s.transform;
          if (t === 'none' || t === '') return true;
          // matrix(1, 0, 0, 1, tx, ty): settled when the X translation is ~0.
          var m = t.match(/matrix\\(([^)]+)\\)/);
          if (!m) return true;
          var parts = m[1].split(',').map(function(n){ return parseFloat(n); });
          return Math.abs(parts[4]) < 0.5;
        })()`),
      { timeout: 2000 },
    )
    .toBeTruthy()
}

/**
 * Stages, captures, and records ONE TOAST surface, isolating its failure like
 * `captureSurface`.
 *
 * Toasts are SNAPSHOT-RESOLVED: their text is resolved once via `tString('key')`
 * at emit time and stored as a plain string, so a later `rerender()` never
 * re-resolves it and never records the key. The recording hook only fires the
 * key if capture is ACTIVE the moment the action emits the toast. So the flow is:
 * reset + setSurface + enable the sink, THEN run `trigger` (the keypress / command
 * that emits the toast), wait for the `.toast` to appear, screenshot, dump. No
 * `rerender` (it can't recover a key resolved before enable, and re-resolving
 * mounted markup would pollute the toast surface with unrelated keys).
 *
 * `trigger` returns nothing; the toast appearance is the readiness signal. After
 * the shot every toast is dismissed so the next surface (and the afterEach leak
 * guard) starts clean.
 */
export async function captureToastSurface(
  label: string,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  main: TauriPage,
  trigger: () => Promise<void>,
): Promise<void> {
  const screenshot = `${label}.png`
  try {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', label)
    await captureCall<boolean>(main, 'enable')
    // Worst-case pass: stage max zoom + min window BEFORE the trigger so the toast
    // renders into the stressed layout. `setTextSize` writes the setting directly
    // (not the `view.zoom.*` command), so it does NOT emit the zoom-change toast
    // that would pollute this surface. No-op outside the worst-case pass.
    await stressLayoutIfWorstCase(main, 'main')
    // Bring the main window foreground BEFORE the trigger so the toast's CSS
    // enter-animation actually composites (an occluded window pauses rAF, leaving
    // the toast stuck at opacity 0 — `waitForToastSettled` would then time out).
    await focusWindow(main, 'main').catch(() => {})
    await trigger()
    // The toast appearing IS the readiness signal: the key was resolved (and so
    // recorded) at emit time, which is inside `trigger`.
    await main.waitForSelector('.toast', 5000)
    // The toast slides in over a 0.2s animation (opacity 0->1, translateX 20->0).
    // `waitForSelector` returns the instant it's in the DOM (mid-animation), so
    // wait for the enter animation to FINISH (opacity 1, transform settled to
    // identity) before the native capture, which composites the last frame and
    // would otherwise grab a half-faded or already-gone toast.
    await waitForToastSettled(main)
    // Whatever is in the toast layer NOW is what this surface staged; `shoot`
    // treats anything else that turns up as a stray and re-shoots. Reading it here
    // (rather than counting) means an unrelated device toast can't pass as ours.
    const expectedToasts = await readToastSignature(main)
    // Crop to the toast layer: a toast is a small card in a corner, and the file
    // panes behind it are noise a translator has to hunt through.
    await shoot(main, 'main', screenshot, { expectedToasts, cropSelector: '.toast-container' })
    report[label] = { screenshot, keys: await keysFor(main, label) }
    await scanForClipping(main, label)
    console.log(`[i18n-capture] ${label}: ${String(report[label].keys.length)} keys → ${screenshot}`)
  } catch (err) {
    failed.push(label)
    console.warn(`[i18n-capture] surface ${label} FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    await dismissAllToasts(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})
  }
}

/**
 * Captures ONE real friendly-error pane as the REPRESENTATIVE image for the whole
 * `errors.*` family (listing / write / provider / git). Every friendly error
 * shares this presentation (a bold title, an explanation paragraph, and a
 * suggestion), so a single honest capture, plus the coupler's representative
 * `@key.screenshotNote`, lets a translator load one image for the entire family.
 *
 * Like a toast, the error copy is SNAPSHOT-RESOLVED: `renderListingError` calls
 * `getMessage('errors.listing.<reason>.*')` once at navigation time and stores
 * plain strings on the FriendlyError props, so a later `rerender()` never
 * re-records them. The sink must be enabled BEFORE the error renders. Flow:
 * reset + setSurface + enable, THEN inject a real OS error (EACCES) and navigate
 * into a subdir so the backend listing fails and the pane renders, capturing the
 * `errors.listing.*` keys it resolves. We screenshot the real pane, then navigate
 * back so the next surface (and the afterEach leak guard) starts clean.
 *
 * Uses the `inject_listing_error` Tauri command (feature-gated behind
 * `playwright-e2e`, present in the capture build): the same hook
 * `error-pane.spec.ts` uses. The injected error is single-shot, so the cleanup
 * navigation succeeds naturally.
 */
export async function captureErrorPaneExample(
  label: string,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  main: TauriPage,
): Promise<void> {
  const screenshot = `${label}.png`
  const fixtureRoot = getFixtureRoot()
  const subDirPath = `${fixtureRoot}/left/sub-dir`
  const leftPath = `${fixtureRoot}/left`
  try {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', label)
    await captureCall<boolean>(main, 'enable')

    // Inject EACCES (errno 13 → a friendly "No permission" error) and navigate
    // into sub-dir in one atomic step (no wait between): a background listing
    // could otherwise consume the single-shot injected error first.
    await main.evaluate(
      `window.__TAURI_INTERNALS__.invoke('inject_listing_error', { volumeId: 'root', errorCode: 13 })`,
    )
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'mcp-nav-to-path',
      payload: { pane: 'left', path: ${JSON.stringify(subDirPath)} }
    })`)
    // The error pane appearing IS the readiness signal: the keys were resolved
    // (and recorded) during the listing the navigation kicked off.
    await main.waitForSelector('.error-pane', 5000)
    // Worst-case pass: max zoom + min window so the error title/explanation/
    // suggestion fight the tightest pane. No-op outside the worst-case pass.
    await stressLayoutIfWorstCase(main, 'main')
    await shoot(main, 'main', screenshot)
    report[label] = { screenshot, keys: await keysFor(main, label) }
    await scanForClipping(main, label)
    console.log(`[i18n-capture] ${label}: ${String(report[label].keys.length)} keys → ${screenshot}`)
  } catch (err) {
    failed.push(label)
    console.warn(`[i18n-capture] surface ${label} FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    await captureCall(main, 'disable').catch(() => {})
    // Navigate back to a real directory so the pane leaves the error state.
    await main
      .evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'mcp-nav-to-path',
        payload: { pane: 'left', path: ${JSON.stringify(leftPath)} }
      })`)
      .catch(() => {})
    await main.waitForSelector('.file-entry', 5000).catch(() => {})
  }
}
