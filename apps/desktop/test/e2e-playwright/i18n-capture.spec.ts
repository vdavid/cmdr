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
  openViewerWindow,
  closeScopedWindow,
  dispatchMenuCommand,
  MKDIR_DIALOG,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/**
 * Every Settings section to capture, beyond the default Appearance one already
 * shot above. `path` is the English section identity the production
 * `navigate-to-section` deep-link takes (NOT the localized sidebar title), so
 * passing the full SUBSECTION path lands on real content rather than a parent's
 * summary-card grid. `sectionId` is the stable `data-section-id` on the rendered
 * `<section>` (see `SettingsContent.svelte`) used as the per-section readiness
 * signal; `label` is the capture surface name. Mirrors the section table in
 * `accessibility.spec.ts` and `SettingsContent.svelte` — keep in sync if a
 * section is added, renamed, or re-homed.
 */
const SETTINGS_SECTIONS: { path: string[]; sectionId: string; label: string }[] = [
  // Appearance › Colors and formats is already captured as `settings-appearance`.
  { path: ['Appearance', 'Zoom and density'], sectionId: 'appearance-zoom-and-density', label: 'settings-appearance-zoom' },
  {
    path: ['Appearance', 'File and folder sizes'],
    sectionId: 'appearance-file-and-folder-sizes',
    label: 'settings-appearance-sizes',
  },
  { path: ['Appearance', 'Listing'], sectionId: 'appearance-listing', label: 'settings-appearance-listing' },
  { path: ['Behavior', 'File operations'], sectionId: 'behavior-file-operations', label: 'settings-behavior-file-operations' },
  {
    path: ['Behavior', 'File system watching'],
    sectionId: 'behavior-file-system-watching',
    label: 'settings-behavior-file-system-watching',
  },
  { path: ['Behavior', 'Search'], sectionId: 'behavior-search', label: 'settings-behavior-search' },
  { path: ['AI'], sectionId: 'ai', label: 'settings-ai' },
  {
    path: ['File systems', 'SMB/Network shares'],
    sectionId: 'file-systems-smb-network-shares',
    label: 'settings-file-systems-smb',
  },
  {
    path: ['File systems', 'MTP (Android/Kindle/cameras)'],
    sectionId: 'file-systems-mtp-android-kindle-cameras',
    label: 'settings-file-systems-mtp',
  },
  { path: ['File systems', 'Git'], sectionId: 'file-systems-git', label: 'settings-file-systems-git' },
  { path: ['Viewer'], sectionId: 'viewer', label: 'settings-viewer' },
  { path: ['Keyboard shortcuts'], sectionId: 'keyboard-shortcuts', label: 'settings-keyboard-shortcuts' },
  { path: ['Developer', 'MCP server'], sectionId: 'developer-mcp-server', label: 'settings-developer-mcp-server' },
  { path: ['Developer', 'Logging'], sectionId: 'developer-logging', label: 'settings-developer-logging' },
  { path: ['Updates & privacy'], sectionId: 'updates', label: 'settings-updates' },
  { path: ['License'], sectionId: 'license', label: 'settings-license' },
  { path: ['Advanced'], sectionId: 'advanced', label: 'settings-advanced' },
]

/**
 * Fixture file the viewer opens. `CMDR_E2E_START_PATH` points at the shared E2E
 * fixture tree (set by the checker before this spec runs); `left/file-a.txt`
 * exists there. Throw rather than fall back to a bogus path so a missing env var
 * surfaces as itself, not a confusing ENOENT in the viewer. Mirrors how
 * `accessibility.spec.ts` resolves its viewer fixture.
 */
function viewerFixturePath(): string {
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
    // ── Surfaces 4..N: every other Settings section (same window) ────────────
    // Reuse the open, focused Settings window and its already-enabled capture
    // sink. For each section, drive the production cross-window deep-link
    // (`navigate-to-section`, the same event the volume picker and shortcut
    // chips use) with the English section PATH, so subsections land on real
    // content instead of a parent's summary grid. Then wait for that section's
    // `data-section-id` to render before switching the surface and shooting.
    // The window stays foreground from the set_focus above, so no re-focus per
    // section; settle one paint after each render to be safe.
    for (const section of SETTINGS_SECTIONS) {
      const sectionJson = JSON.stringify({ section: section.path })
      await settings.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit_to', {
        target: { kind: 'AnyLabel', label: 'settings' },
        event: 'navigate-to-section',
        payload: ${sectionJson}
      })`)
      await settings.waitForSelector(`[data-section-id="${section.sectionId}"]`, 5000)
      await captureCall(settings, 'setSurface', section.label)
      await captureCall(settings, 'rerender')
      await settlePaint(settings)
      await settings.screenshot({ path: join(screenshotsDir, `${section.label}.png`) })
      report[section.label] = {
        screenshot: `${section.label}.png`,
        keys: await keysFor(settings, section.label),
      }
    }
    await captureCall(settings, 'disable')
    await closeScopedWindow(main, settings, 'settings')

    // ── Surface: file viewer window ──────────────────────────────────────────
    // The viewer runs in its own restricted WebviewWindow (own webview JS
    // context, own capture sink). Open it on a real fixture file via the
    // production `open-file-viewer` event (the `openViewerWindow` helper), wait
    // for the container to report `loaded`, then enable + capture against the
    // VIEWER page. Just the default text-viewer chrome (toolbar + status bar);
    // media/search subsurfaces are a later tranche.
    const viewer = await openViewerWindow(main, viewerFixturePath())
    await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 15000)
    await captureCall(viewer, 'reset')
    await captureCall<boolean>(viewer, 'enable')
    await captureCall(viewer, 'setSurface', 'viewer')
    await captureCall(viewer, 'rerender')
    // Same occluded-window compositing caveat as Settings: focus the viewer so
    // its backing store (which the native capture reads) refreshes to the
    // current frame, then settle one paint. The viewer label is `viewer-<ts>`,
    // resolved from the scoped page (set by `waitForWindow`).
    const viewerLabel = viewer.targetWindow
    if (!viewerLabel) throw new Error('viewer page has no targetWindow label')
    const viewerLabelJson = JSON.stringify(viewerLabel)
    await viewer.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_focus', { label: ${viewerLabelJson} })`)
    await settlePaint(viewer)
    await viewer.screenshot({ path: join(screenshotsDir, 'viewer.png') })
    report['viewer'] = { screenshot: 'viewer.png', keys: await keysFor(viewer, 'viewer') }
    await captureCall(viewer, 'disable')
    await closeScopedWindow(main, viewer, viewerLabel)

    // ── Surface: keyboard shortcuts window ───────────────────────────────────
    // A separate singleton WebviewWindow (label `shortcuts`) on the `/shortcuts`
    // route, opened by the `help.openShortcuts` command (same path Help >
    // Keyboard shortcuts uses). There's no dedicated E2E helper, so dispatch the
    // command and poll the window list for the `shortcuts` label, mirroring how
    // `openSettingsWindowViaProd` waits for its window.
    await dispatchMenuCommand(main, 'help.openShortcuts')
    const shortcuts = await main.waitForWindow((w) => w.label === 'shortcuts', { timeout: 10000 })
    await shortcuts.waitForSelector('.shortcuts-window', 5000)
    // Wait for real content (the command list), not just the window shell.
    await shortcuts.waitForSelector('.shortcuts-scroll .row', 5000)
    await captureCall(shortcuts, 'reset')
    await captureCall<boolean>(shortcuts, 'enable')
    await captureCall(shortcuts, 'setSurface', 'shortcuts')
    await captureCall(shortcuts, 'rerender')
    await shortcuts.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_focus', { label: 'shortcuts' })`)
    await settlePaint(shortcuts)
    await shortcuts.screenshot({ path: join(screenshotsDir, 'shortcuts.png') })
    report['shortcuts'] = { screenshot: 'shortcuts.png', keys: await keysFor(shortcuts, 'shortcuts') }
    await captureCall(shortcuts, 'disable')
    await closeScopedWindow(main, shortcuts, 'shortcuts')

    // ── Surface: About dialog (main window overlay) ──────────────────────────
    // About is an in-app dialog rendered into the MAIN window (NOT a separate
    // WebviewWindow), so it captures against the main page's sink. Open it via
    // the `app.about` command (Help > About Cmdr), wait for the dialog, then
    // re-enable capture on the main page, set the surface, rerender, and shoot.
    // The main window is foreground, so a paint settle (as for the new-folder
    // dialog above) is enough — no set_focus needed.
    await captureCall(main, 'setSurface', 'about')
    await captureCall<boolean>(main, 'enable')
    await dispatchMenuCommand(main, 'app.about')
    await main.waitForSelector('[data-dialog-id="about"]', 5000)
    await captureCall(main, 'rerender')
    await settlePaint(main)
    await main.screenshot({ path: join(screenshotsDir, 'about.png') })
    report['about'] = { screenshot: 'about.png', keys: await keysFor(main, 'about') }
    await dismissOverlay(main)
    await captureCall(main, 'disable')

    writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n')
    // Surface a compact summary in the test output for quick eyeballing.
    for (const [surface, data] of Object.entries(report)) {
      console.log(`[i18n-capture] ${surface}: ${String(data.keys.length)} keys → ${data.screenshot}`)
    }
    console.log(`[i18n-capture] report written to ${reportPath}`)
  })
})
