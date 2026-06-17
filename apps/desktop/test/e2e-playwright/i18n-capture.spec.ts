/**
 * i18n screenshot-capture driver.
 *
 * NOT a pass/fail test: a harness that drives the real app to a set of surfaces
 * and, for each, records which catalog keys render there (via the runtime's
 * capture mode in `$lib/intl/messages.svelte.ts`) and saves a native screenshot.
 * The output is a single JSON map (surface → keys + screenshot file) that
 * `scripts/couple-screenshots.js` turns into `@key.screenshot` couplings.
 *
 * Run it like any single spec (the app must already be running; see the suite's
 * DETAILS.md § "Running a single spec"), or via `pnpm i18n:capture` which builds,
 * launches, runs only this spec, and tears the app down. It's excluded from the
 * normal E2E lanes by filename (`grepInvert` in playwright.config.ts) so a full
 * suite run doesn't spend time taking screenshots.
 *
 * Coupling policy: a key may render on several surfaces; the coupler assigns each
 * key the FIRST surface (in this file's order) it appeared on, so the most
 * specific / smallest surface that a key belongs to wins when ordered narrow-to-
 * broad below. Keep the surface order intentional.
 */

import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from './fixtures.js'
import {
  ensureAppReady,
  dismissOverlay,
  skipParentEntry,
  openSettingsWindowViaProd,
  closeScopedWindow,
  MKDIR_DIALOG,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

interface CaptureApi {
  enable: () => boolean
  disable: () => void
  setSurface: (label: string) => void
  dump: () => Record<string, string[]>
  reset: () => void
  rerender: () => void
}

const here = dirname(fileURLToPath(import.meta.url))
const screenshotsDir = join(here, '..', '..', 'src', 'lib', 'intl', 'messages', 'screenshots')
const reportPath = join(screenshotsDir, 'capture-report.json')

/** Calls a method on the webview's `window.__cmdrI18nCapture`, returns its result. */
async function captureCall<T>(page: TauriPage, method: keyof CaptureApi, arg?: string): Promise<T> {
  const argJson = arg === undefined ? '' : JSON.stringify(arg)
  return page.evaluate<T>(`(function() {
    var api = window.__cmdrI18nCapture;
    if (!api) throw new Error('__cmdrI18nCapture not installed; build with playwright-e2e and ensure non-prod mode');
    return api.${method}(${argJson});
  })()`)
}

/** Catalog keys recorded for `surface`, sorted, read from the live sink. */
async function keysFor(page: TauriPage, surface: string): Promise<string[]> {
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
async function settlePaint(page: TauriPage): Promise<void> {
  await page.evaluate(`new Promise(function(resolve) {
    var done = false;
    var finish = function() { if (!done) { done = true; resolve(true); } };
    requestAnimationFrame(function() { requestAnimationFrame(finish); });
    setTimeout(finish, 500);
  })`)
}

test.describe('i18n screenshot capture', () => {
  // This drives three surfaces (main, a dialog, a separate Settings window) with
  // window open/close, so it legitimately needs longer than the 15s per-test
  // default. (A normal interaction test should fit in 15s; this is a multi-surface
  // capture driver, not a normal test.)
  test('captures representative surfaces and writes the coupling report', async ({ tauriPage }) => {
    test.setTimeout(60000)
    const main = tauriPage as TauriPage
    mkdirSync(screenshotsDir, { recursive: true })

    // The fixture auto-starts a video recorder (15 fps frame capture). It's
    // useless for this driver and just burns CPU + CoreGraphics work alongside
    // the screenshots, so stop it up front. Best-effort: never fail the run on it.
    try {
      await (main as unknown as { stopRecording: () => Promise<unknown> }).stopRecording()
    } catch {
      // Already stopped or unsupported — fine.
    }

    // surface label → { keys, screenshot filename }
    const report: Record<string, { screenshot: string; keys: string[] }> = {}

    // The capture flow per surface: stage it (so its markup is mounted), set the
    // surface label, then `rerender()` — bumping the locale-version rune forces
    // every reactive `t()`/`<Trans>` in the mounted markup to re-run with NO
    // visible change, recording each resolved key under the current surface. This
    // works uniformly whether the surface mounted before or after capture started.

    // ── Surface 1: main dual-pane window ─────────────────────────────────────
    await ensureAppReady(main)
    await main.waitForSelector('.file-entry', 5000)
    await captureCall(main, 'reset')
    await captureCall<boolean>(main, 'enable')
    await captureCall(main, 'setSurface', 'main-window')
    await captureCall(main, 'rerender')
    await main.screenshot({ path: join(screenshotsDir, 'main-window.png') })
    report['main-window'] = { screenshot: 'main-window.png', keys: await keysFor(main, 'main-window') }

    // ── Surface 2: new-folder dialog (F7) ────────────────────────────────────
    await skipParentEntry(main)
    await main.keyboard.press('F7')
    await main.waitForSelector(MKDIR_DIALOG, 5000)
    await main.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await captureCall(main, 'setSurface', 'new-folder-dialog')
    await captureCall(main, 'rerender')
    // The modal renders into the foreground main window; settle one paint so the
    // native capture includes it (it's absent otherwise — the capture grabs the
    // pre-modal composited frame).
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, 'new-folder-dialog.png') })
    report['new-folder-dialog'] = {
      screenshot: 'new-folder-dialog.png',
      keys: await keysFor(main, 'new-folder-dialog'),
    }
    await dismissOverlay(main)
    await captureCall(main, 'disable')

    // ── Surface 3: Settings window (default Appearance section) ───────────────
    // Settings runs in its own Tauri WebviewWindow, hence its own webview JS
    // context with its own `__cmdrI18nCapture` and its own sink. So enable +
    // setSurface + rerender + dump all run against the SETTINGS page.
    const settings = await openSettingsWindowViaProd(main)
    await settings.waitForSelector('.settings-window', 5000)
    // Wait for the actual Appearance CONTENT, not just the window shell: the
    // window briefly shows "Loading settings..." before the section renders, and
    // capturing the shell catches that placeholder. The first appearance section
    // is the readiness signal that real content is on screen.
    await settings.waitForSelector('[data-section-id="appearance-colors-and-formats"]', 5000)
    await captureCall(settings, 'reset')
    await captureCall<boolean>(settings, 'enable')
    await captureCall(settings, 'setSurface', 'settings-appearance')
    await captureCall(settings, 'rerender')
    // Bring the Settings window to the front before capturing. In E2E child
    // windows are ordered to the BACK (so a dev's work isn't disturbed), but macOS
    // throttles/pauses compositing for an occluded window — so its backing store,
    // which the native capture reads, stays frozen on the "Loading settings..."
    // placeholder even after the real content is in the DOM. Focusing it makes it
    // composite the current frame. (`core:window:allow-set-focus` is granted in
    // the settings capability.) Then settle one paint before the shot.
    await settings.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_focus', { label: 'settings' })`)
    await settlePaint(settings)
    await settings.screenshot({ path: join(screenshotsDir, 'settings-appearance.png') })
    report['settings-appearance'] = {
      screenshot: 'settings-appearance.png',
      keys: await keysFor(settings, 'settings-appearance'),
    }
    await captureCall(settings, 'disable')
    await closeScopedWindow(main, settings, 'settings')

    writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n')
    // Surface a compact summary in the test output for quick eyeballing.
    for (const [surface, data] of Object.entries(report)) {
      console.log(`[i18n-capture] ${surface}: ${String(data.keys.length)} keys → ${data.screenshot}`)
    }
    console.log(`[i18n-capture] report written to ${reportPath}`)
  })
})
