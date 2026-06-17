/**
 * Shared capture machinery for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * Holds the reusable primitives every surface group leans on: the capture-sink
 * RPC (`captureCall` / `keysFor`), the paint/focus settling helpers, the
 * report-path constants, the shared types, and the two surface-capturing engines
 * (`captureSurface` for reactive mounted markup, `captureToastSurface` for
 * snapshot-resolved toasts). The per-group capture functions live in
 * `i18n-capture-surfaces.ts` and the orchestration in the spec; both import from
 * here. Split out purely for the file-length budget — behavior is unchanged.
 */

import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect } from './fixtures.js'
import { getFixtureRoot } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/**
 * Fixture file the viewer opens. `CMDR_E2E_START_PATH` points at the shared E2E
 * fixture tree (set by the checker before this spec runs); `left/file-a.txt`
 * exists there. Throw rather than fall back to a bogus path so a missing env var
 * surfaces as itself, not a confusing ENOENT in the viewer. Mirrors how
 * `accessibility.spec.ts` resolves its viewer fixture.
 */
export function viewerFixturePath(): string {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root) {
    throw new Error('CMDR_E2E_START_PATH env var is not set; fixtures must be created before running this spec')
  }
  return join(root, 'left', 'file-a.txt')
}

interface CaptureApi {
  enable: () => boolean
  disable: () => void
  setSurface: (label: string) => void
  dump: () => Record<string, string[]>
  reset: () => void
  rerender: () => void
}

const here = dirname(fileURLToPath(import.meta.url))
export const screenshotsDir = join(here, '..', '..', 'src', 'lib', 'intl', 'messages', 'screenshots')
export const reportPath = join(screenshotsDir, 'capture-report.json')
/** Sibling list of surfaces that FAILED to capture this run (coverage honesty). */
export const failedPath = join(screenshotsDir, 'capture-failed.json')
/** Sibling list of surfaces deliberately SKIPPED (documented harness gaps). */
export const skippedPath = join(screenshotsDir, 'capture-skipped.json')

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
 * Resolves on the next animation frame, BUT races a short timeout: `requestAnimationFrame`
 * is throttled/paused on a window that isn't foreground (in E2E, child windows
 * are ordered to the back), where it would otherwise never fire and hang the
 * eval. The timeout is a safety net, not the primary signal — a foreground window
 * resolves on the real frame in ~16 ms.
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
 * Brings a separate window frontmost via `plugin:window|set_focus`. Needed both
 * to unstall a window's occluded-throttled async `onMount` (settings/shortcuts
 * gate content on it) and so macOS composites the current frame for the native
 * screenshot. `core:window:allow-set-focus` is granted in each window's
 * capability.
 */
export async function focusWindow(page: TauriPage, label: string): Promise<void> {
  const labelJson = JSON.stringify(label)
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_focus', { label: ${labelJson} })`)
}

/** A surface's report entry: the screenshot file and the keys recorded for it. */
export interface SurfaceEntry {
  screenshot: string
  keys: string[]
}

/** What a surface's `stage` step hands back to `captureSurface`. */
export interface StagedSurface {
  /** The page whose capture sink + screenshot this surface uses. */
  page: TauriPage
  /**
   * Window label to `set_focus` before the shot. Separate (occluded) windows
   * need it so macOS composites the current frame into the backing store the
   * native capture reads. Omit for overlays on the already-foreground main
   * window (the new-folder + About dialogs).
   */
  focusLabel?: string
}

/**
 * Stages, captures, and records ONE surface, isolating its failure: any throw is
 * caught, logged, and pushed to `failed`, and the run continues to the next
 * surface. Without this isolation a single broken surface (e.g. a window that
 * won't load) aborts the whole driver before the report is written, discarding
 * every surface that already succeeded — fatal whack-a-mole for a ~50-surface
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
    const { page, focusLabel } = await stage()
    await captureCall(page, 'setSurface', label)
    await captureCall(page, 'rerender')
    if (focusLabel !== undefined) await focusWindow(page, focusLabel)
    await settlePaint(page)
    await page.screenshot({ path: join(screenshotsDir, screenshot) })
    report[label] = { screenshot, keys: await keysFor(page, label) }
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

/** Closes every open toast by clicking its `.toast-close`, then waits for them to clear. */
async function dismissAllToasts(page: TauriPage): Promise<void> {
  await page.evaluate(`(function(){
    var toasts = document.querySelectorAll('.toast');
    for (var i = 0; i < toasts.length; i++) {
      var close = toasts[i].querySelector('.toast-close');
      if (close) close.click();
    }
  })()`)
  await expect.poll(async () => (await page.count('.toast')) === 0, { timeout: 3000 }).toBeTruthy()
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
    await trigger()
    // The toast appearing IS the readiness signal: the key was resolved (and so
    // recorded) at emit time, which is inside `trigger`.
    await main.waitForSelector('.toast', 5000)
    // The toast slides in over a 0.2s animation (opacity 0→1, translateX 20→0).
    // `waitForSelector` returns the instant it's in the DOM — mid-animation — so
    // wait for the enter animation to FINISH (opacity 1, transform settled to
    // identity) before the native capture, which composites the last frame and
    // would otherwise grab a half-faded or already-gone toast.
    await waitForToastSettled(main)
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, screenshot) })
    report[label] = { screenshot, keys: await keysFor(main, label) }
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
 * shares this presentation — a bold title, an explanation paragraph, and a
 * suggestion — so a single honest capture, plus the coupler's representative
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
 * `playwright-e2e`, present in the capture build) — the same hook
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
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, screenshot) })
    report[label] = { screenshot, keys: await keysFor(main, label) }
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
