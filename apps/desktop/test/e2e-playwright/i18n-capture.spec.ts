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
 * The native (CoreGraphics) screenshot grabs the window's last COMPOSITED frame,
 * which lags a just-applied DOM change (a freshly-opened modal, a `rerender()`).
 * Two rAF ticks guarantee the browser has laid out AND painted the pending
 * change, so the capture shows the surface we actually staged. Without this, a
 * modal that's present in the DOM can be missing from the image.
 */
async function settlePaint(page: TauriPage): Promise<void> {
  await page.evaluate(`new Promise(function(resolve) {
    requestAnimationFrame(function() { requestAnimationFrame(function() { resolve(true); }); });
  })`)
}

test.describe('i18n screenshot capture', () => {
  test('captures representative surfaces and writes the coupling report', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    mkdirSync(screenshotsDir, { recursive: true })

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
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, 'main-window.png') })
    report['main-window'] = { screenshot: 'main-window.png', keys: await keysFor(main, 'main-window') }

    // ── Surface 2: new-folder dialog (F7) ────────────────────────────────────
    await skipParentEntry(main)
    await main.keyboard.press('F7')
    await main.waitForSelector(MKDIR_DIALOG, 5000)
    await main.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await captureCall(main, 'setSurface', 'new-folder-dialog')
    await captureCall(main, 'rerender')
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
    await settings.waitForSelector('.settings-sidebar', 5000)
    await captureCall(settings, 'reset')
    await captureCall<boolean>(settings, 'enable')
    await captureCall(settings, 'setSurface', 'settings-appearance')
    await captureCall(settings, 'rerender')
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
