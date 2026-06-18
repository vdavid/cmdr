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
 * here. Split out purely for the file-length budget; behavior is unchanged.
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
  setLocale: (tag: string | null) => void
  setTextSize: (percent: number) => Promise<void>
}

const here = dirname(fileURLToPath(import.meta.url))
const baseScreenshotsDir = join(here, '..', '..', 'src', 'lib', 'intl', 'messages', 'screenshots')

/**
 * The locale this run captures in for OVERFLOW review (the pseudolocale `en-XA`),
 * or empty for the normal English coupling capture. Set by the orchestrator via
 * `CMDR_I18N_OVERFLOW_LOCALE`. When set, the run is an overflow pass: screenshots
 * land in a SEPARATE `overflow/` dir (so they never overwrite the coupling
 * screenshots), the driver switches the app to this locale before capturing, and
 * each surface gets a DOM clip-overflow scan. An overflow pass never touches the
 * coupling artifacts (`capture-report.json` / `@key.screenshot`).
 */
export const overflowLocale = process.env.CMDR_I18N_OVERFLOW_LOCALE ?? ''
export const isOverflowPass = overflowLocale !== ''

/**
 * Worst-case overflow pass (overflow pass only): on top of the pseudolocale, the
 * driver maxes the UI zoom (`MAX_UI_ZOOM`) and resizes each captured window to
 * its minimum allowed size before the shot + clip scan, the maximal-overflow
 * scenario a translator must fit. Set by the orchestrator via
 * `CMDR_I18N_WORST_CASE`. No effect outside an overflow pass.
 */
export const isWorstCasePass = isOverflowPass && process.env.CMDR_I18N_WORST_CASE === '1'

/**
 * The largest UI zoom the app offers (the `appearance.textSize` percentage; the
 * `view.zoom.set150` preset is the ceiling). The worst-case pass drives the app
 * to this before capturing so layout is stressed at max zoom AND inflated text.
 */
export const MAX_UI_ZOOM = 150

/**
 * Where screenshots land this run: the coupling dir for a normal pass, a
 * dedicated `overflow/` subdir for an overflow pass, and a further
 * `overflow/worst-case/` subdir for the worst-case pass (all gitignored), so the
 * three never overwrite each other.
 */
export const screenshotsDir = isWorstCasePass
  ? join(baseScreenshotsDir, 'overflow', 'worst-case')
  : isOverflowPass
    ? join(baseScreenshotsDir, 'overflow')
    : baseScreenshotsDir
export const reportPath = join(screenshotsDir, 'capture-report.json')
/** Sibling list of surfaces that FAILED to capture this run (coverage honesty). */
export const failedPath = join(screenshotsDir, 'capture-failed.json')
/** Sibling list of surfaces deliberately SKIPPED (documented harness gaps). */
export const skippedPath = join(screenshotsDir, 'capture-skipped.json')

/**
 * One element flagged by the clip-overflow scan: a text-bearing node whose
 * content is cut off by its own box (its scroll size exceeds its client size
 * while `overflow` clips). Best-effort heuristic, not proof of a visible defect.
 */
export interface ClipFinding {
  /** A short CSS-ish path to the element (tag + id/classes), for the report. */
  selector: string
  /** The clipped text content (trimmed, capped), so the reviewer can spot it. */
  text: string
  /** Horizontal overflow in px (`scrollWidth - clientWidth`), 0 if none. */
  overflowX: number
  /** Vertical overflow in px (`scrollHeight - clientHeight`), 0 if none. */
  overflowY: number
}

/** surface label → the clip findings detected on it (empty array = clean). */
export const clipFindings: Record<string, ClipFinding[]> = {}

/**
 * Scans the page's DOM for text that its own box clips, and records the findings
 * under `label`. The heuristic: a text-bearing element whose `scrollWidth >
 * clientWidth` (or `scrollHeight > clientHeight`) by more than a small tolerance,
 * AND whose computed `overflow` in that axis hides/clips the spill (so the text
 * is actually cut off, not scrollable into view). We skip naturally-scrollable
 * containers (`auto`/`scroll`), visually-hidden accessibility nodes (`sr-only` /
 * the announcer, which always clip by design), and elements with no direct text.
 * This finds the common pseudolocale failures: a truncated button/label/header
 * where +40% text no longer fits. It is a HEURISTIC: it can miss a clip that an
 * ancestor masks, and can flag a deliberately-ellipsized label (which may be
 * acceptable design). Treat the report as a list of spots to eyeball, not a hard
 * pass/fail. No-op outside an overflow pass.
 */
export async function scanForClipping(page: TauriPage, label: string): Promise<void> {
  if (!isOverflowPass) return
  try {
    const findings = await page.evaluate<ClipFinding[]>(`(function() {
      var TOL = 1; // sub-pixel rounding tolerance
      var out = [];
      var nodes = document.querySelectorAll('body *');
      for (var i = 0; i < nodes.length; i++) {
        var el = nodes[i];
        // Only text-bearing elements: at least one direct, non-whitespace text node.
        var hasText = false;
        for (var c = 0; c < el.childNodes.length; c++) {
          var n = el.childNodes[c];
          if (n.nodeType === 3 && n.textContent && n.textContent.trim() !== '') { hasText = true; break; }
        }
        if (!hasText) continue;
        var s = getComputedStyle(el);
        if (s.display === 'none' || s.visibility === 'hidden' || parseFloat(s.opacity) === 0) continue;
        // Skip visually-hidden accessibility nodes: the standard 'sr-only' /
        // screen-reader-announcer pattern collapses the box to a 1px clip-rect, so
        // it ALWAYS "clips" its text by design and is never seen by a user. Flagging
        // it is pure noise that buries real overflow. Detect it by the conventional
        // class names AND by the tell-tale tiny clip box (clientW/H <= 1px).
        var cls = (typeof el.className === 'string') ? el.className : '';
        if (/\\bsr-only\\b/.test(cls) || el.id === 'svelte-announcer') continue;
        if (el.clientWidth <= 1 || el.clientHeight <= 1) continue;
        var ofx = el.scrollWidth - el.clientWidth;
        var ofy = el.scrollHeight - el.clientHeight;
        // Only count an axis whose overflow is hidden/clipped/ellipsed (text is
        // actually cut off). 'auto'/'scroll' means the user can reach it, 'visible'
        // means it spills (a layout-break, caught separately below).
        var clipsX = (s.overflowX === 'hidden' || s.overflowX === 'clip' || s.textOverflow === 'ellipsis');
        var clipsY = (s.overflowY === 'hidden' || s.overflowY === 'clip');
        var hitX = ofx > TOL && clipsX;
        var hitY = ofy > TOL && clipsY;
        if (!hitX && !hitY) continue;
        // Build a short selector for the report.
        var sel = el.tagName.toLowerCase();
        if (el.id) sel += '#' + el.id;
        if (cls) {
          var selCls = cls.trim().split(/\\s+/).slice(0, 3).join('.');
          if (selCls) sel += '.' + selCls;
        }
        var txt = (el.textContent || '').trim().replace(/\\s+/g, ' ');
        if (txt.length > 80) txt = txt.slice(0, 80) + '…';
        out.push({ selector: sel, text: txt, overflowX: hitX ? ofx : 0, overflowY: hitY ? ofy : 0 });
      }
      // De-dup identical (selector,text) rows an ancestor + child can both produce.
      var seen = {};
      var dedup = [];
      for (var k = 0; k < out.length; k++) {
        var key = out[k].selector + '|' + out[k].text;
        if (seen[key]) continue;
        seen[key] = true;
        dedup.push(out[k]);
      }
      return dedup;
    })()`)
    clipFindings[label] = findings
    if (findings.length > 0) {
      console.warn(`[i18n-overflow] ${label}: ${String(findings.length)} clipped element(s)`)
    }
  } catch {
    // Best-effort: a window whose eval channel is unresponsive (e.g. shortcuts)
    // just gets no findings rather than failing the run.
    clipFindings[label] ??= []
  }
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
 * Resolves on the next animation frame, BUT races a short timeout: `requestAnimationFrame`
 * is throttled/paused on a window that isn't foreground (in E2E, child windows
 * are ordered to the back), where it would otherwise never fire and hang the
 * eval. The timeout is a safety net, not the primary signal: a foreground window
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
 * and `shortcuts-window.ts` (`MIN_WIDTH*scale` by `MIN_HEIGHT`). The scaled ones
 * use the worst-case scale (`MAX_UI_ZOOM/100`).
 */
function minSizeFor(label: string): { width: number; height: number } {
  const scale = MAX_UI_ZOOM / 100
  if (label === 'settings') return { width: 252 + 348 * scale, height: 400 * scale }
  if (label === 'shortcuts') return { width: 300 * scale, height: 420 }
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
  await captureCall(page, 'setTextSize', String(MAX_UI_ZOOM))
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
    const { page, focusLabel } = await stage()
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
    await stressLayoutIfWorstCase(page, focusLabel ?? 'main')
    if (focusLabel !== undefined) await focusWindow(page, focusLabel)
    await settlePaint(page)
    await page.screenshot({ path: join(screenshotsDir, screenshot) })
    report[label] = { screenshot, keys: await keysFor(page, label) }
    await scanForClipping(page, label)
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
    // Worst-case pass: stage max zoom + min window BEFORE the trigger so the toast
    // renders into the stressed layout. `setTextSize` writes the setting directly
    // (not the `view.zoom.*` command), so it does NOT emit the zoom-change toast
    // that would pollute this surface. No-op outside the worst-case pass.
    await stressLayoutIfWorstCase(main, 'main')
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
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, screenshot) })
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
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, screenshot) })
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
