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
import { test, expect } from './fixtures.js'
import {
  ensureAppReady,
  dismissOverlay,
  skipParentEntry,
  moveCursorToFile,
  openSettingsWindowViaProd,
  openViewerWindow,
  closeScopedWindow,
  dispatchMenuCommand,
  getFixtureRoot,
  LOCAL_VOLUME_NAME,
  MKDIR_DIALOG,
  TRANSFER_DIALOG,
} from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { initMcpClient, mcpSelectVolume } from '../e2e-shared/mcp-client.js'
import { writeFile, waitForConflictPolicy, clickTransferStart } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/**
 * Every Settings section to capture, in capture (coupling) order. `path` is the
 * English section identity the production `navigate-to-section` deep-link takes
 * (NOT the localized sidebar title), so passing the full SUBSECTION path lands on
 * real content rather than a parent's summary-card grid. `sectionId` is the
 * stable `data-section-id` on the rendered `<section>` (see
 * `SettingsContent.svelte`) used as the per-section readiness signal; `label` is
 * the capture surface name. Mirrors the section table in `accessibility.spec.ts`
 * and `SettingsContent.svelte` — keep in sync if a section is added, renamed, or
 * re-homed.
 *
 * EVERY section (including the first, Appearance › Colors and formats) is reached
 * by an explicit deep-link, never by relying on the window's default-rendered
 * section: that default is the last-viewed section restored from the persisted
 * store, which is non-deterministic (a prior session can leave it on "Advanced",
 * and a top-level section like "Appearance" renders a summary grid with no
 * `data-section-id` at all). Deep-linking each one makes the run deterministic.
 */
const SETTINGS_SECTIONS: { path: string[]; sectionId: string; label: string }[] = [
  {
    path: ['Appearance', 'Colors and formats'],
    sectionId: 'appearance-colors-and-formats',
    label: 'settings-appearance',
  },
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
/** Sibling list of surfaces that FAILED to capture this run (coverage honesty). */
const failedPath = join(screenshotsDir, 'capture-failed.json')
/** Sibling list of surfaces deliberately SKIPPED (documented harness gaps). */
const skippedPath = join(screenshotsDir, 'capture-skipped.json')

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

/**
 * Brings a separate window frontmost via `plugin:window|set_focus`. Needed both
 * to unstall a window's occluded-throttled async `onMount` (settings/shortcuts
 * gate content on it) and so macOS composites the current frame for the native
 * screenshot. `core:window:allow-set-focus` is granted in each window's
 * capability.
 */
async function focusWindow(page: TauriPage, label: string): Promise<void> {
  const labelJson = JSON.stringify(label)
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_focus', { label: ${labelJson} })`)
}

/** A surface's report entry: the screenshot file and the keys recorded for it. */
interface SurfaceEntry {
  screenshot: string
  keys: string[]
}

/** What a surface's `stage` step hands back to `captureSurface`. */
interface StagedSurface {
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
async function captureSurface(
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
 * Captures the Settings window's every section.
 *
 * Settings runs in its own Tauri WebviewWindow (own webview JS context, own
 * `__cmdrI18nCapture` sink). Open it ONCE, then drive the production
 * `navigate-to-section` deep-link (the same event the volume picker / shortcut
 * chips use, passing the English section PATH so subsections land on real
 * content, not a parent summary grid) to each section, reusing the one window +
 * sink. Each `captureSurface` re-focuses for the shot so macOS composites the
 * current frame into the backing store the native capture reads.
 *
 * Wrapped so an open failure marks every not-yet-done settings surface failed
 * (rather than throwing) and the window is always closed. Per-surface isolation
 * means one section's failure doesn't stop the rest.
 */
async function captureSettingsWindow(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  let settings: TauriPage | undefined
  try {
    settings = await openSettingsWindowViaProd(main)
    const settingsPage = settings
    await settingsPage.waitForSelector('.settings-window', 5000)
    // The settings page gates content behind `{#if initialized}`, which flips
    // true at the END of an async `onMount`. Focus the window so its async inits
    // aren't throttled while occluded, then wait for `initialized` (the sidebar
    // renders only after it). Don't wait on a specific section here: the
    // default-rendered section is restored from the persisted store and is
    // non-deterministic — the loop below deep-links to each section explicitly.
    await focusWindow(settingsPage, 'settings')
    await settingsPage.waitForSelector('.settings-window .section-item', 10000)
    await captureCall(settingsPage, 'reset')
    await captureCall<boolean>(settingsPage, 'enable')

    for (const section of SETTINGS_SECTIONS) {
      await captureSurface(section.label, report, failed, async () => {
        const sectionJson = JSON.stringify({ section: section.path })
        await settingsPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit_to', {
          target: { kind: 'AnyLabel', label: 'settings' },
          event: 'navigate-to-section',
          payload: ${sectionJson}
        })`)
        await settingsPage.waitForSelector(`[data-section-id="${section.sectionId}"]`, 5000)
        return { page: settingsPage, focusLabel: 'settings' }
      })
    }
    await captureCall(settingsPage, 'disable')
  } catch (err) {
    // Opening the window or the `initialized` wait failed: mark every
    // not-yet-done settings surface failed so the run continues and the report
    // stays honest.
    for (const { label } of SETTINGS_SECTIONS) {
      if (!(label in report) && !failed.includes(label)) failed.push(label)
    }
    console.warn(`[i18n-capture] Settings window setup FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    if (settings) await closeScopedWindow(main, settings, 'settings').catch(() => {})
  }
}

/**
 * Captures every MAIN-WINDOW overlay surface: the file-op dialogs (new file,
 * delete, trash, rename, extension-change, conflict, transfer), the navigation
 * and networking dialogs (go-to-path, connect-to-server), the command palette,
 * and the shared-`QueryDialog` query UI (search, selection, filter popover).
 *
 * All render into the main window's own capture sink, so each follows the About
 * pattern: enable + setSurface the sink BEFORE opening (to record mount-time
 * `t()` calls), open, wait on a per-overlay selector, capture, then dismiss +
 * disable. The `mainOverlay` local wraps that rhythm; its `open` callback does
 * the surface-specific staging and returns the wait selector. Surfaces that
 * mutate the fixture tree (rename, delete, conflict) get a fresh tree up front,
 * and cursor-dependent stages re-skip the synthetic `..` row first.
 *
 * Extracted from the test body so each surface's staging stays small and the
 * driver's top-level complexity stays under the lint ceiling.
 */
async function captureMainOverlays(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await initMcpClient(main)

  // Stages one main-window overlay: enable+setSurface, run the surface-specific
  // `open` (returns its wait selector), let `captureSurface` shoot it, then
  // dismiss + disable. `dismissOverlay` no-ops (caught) for non-overlay surfaces.
  const mainOverlay = async (label: string, open: () => Promise<string>): Promise<void> => {
    await captureSurface(label, report, failed, async () => {
      await captureCall(main, 'reset')
      await captureCall(main, 'setSurface', label)
      await captureCall<boolean>(main, 'enable')
      const waitSelector = await open()
      await main.waitForSelector(waitSelector, 5000)
      return { page: main }
    })
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})
  }

  // New-file dialog (⇧F4 → `file.newFile`). The mkfile twin of `new-folder-dialog`.
  await mainOverlay('new-file-dialog', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.newFile')
    return '[data-dialog-id="new-file-confirmation"] .name-input'
  })

  // Delete confirmation (F8 → `file.delete`): the recycle/trash-style confirm.
  await mainOverlay('delete-confirm', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.delete')
    return '[data-dialog-id="delete-confirmation"]'
  })

  // Permanent-delete confirmation (⇧F8 → `file.deletePermanently`). Same
  // `delete-confirmation` dialog id as trash, but distinct copy (no-trash
  // warning / permanent wording), so it earns its own surface for the keys the
  // trash variant doesn't render.
  await mainOverlay('trash-confirm', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.deletePermanently')
    return '[data-dialog-id="delete-confirmation"]'
  })

  // Rename: the inline editor (F2 → `file.rename`), NOT a modal — the input
  // mounts in-pane, so `dismissOverlay` (which only knows overlay selectors)
  // can't close it. Cancel the editor explicitly with a synthetic Escape.
  await captureSurface('rename-dialog', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'rename-dialog')
    await captureCall<boolean>(main, 'enable')
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.rename')
    await main.waitForSelector('.rename-input', 5000)
    return { page: main }
  })
  await main.evaluate(`(function(){
    var el = document.querySelector('.rename-input');
    if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  })()`)
  await expect.poll(async () => (await main.count('.rename-input')) === 0, { timeout: 3000 }).toBeTruthy()
  await captureCall(main, 'disable').catch(() => {})

  // Extension-change confirmation: rename to a MEANINGFULLY different extension
  // (default `fileOperations.allowFileExtensionChanges` is "ask"). `.txt` → a
  // non-equivalent extension like `.zip` triggers the dialog; equivalent groups
  // (`.txt`/`.md`, `.jpg`/`.jpeg`, …) are silently allowed and would NOT show it.
  // Drive the inline editor to the new extension, then ⏎.
  await mainOverlay('extension-change', async () => {
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-a.txt')
    await dispatchMenuCommand(main, 'file.rename')
    await main.waitForSelector('.rename-input', 3000)
    await main.evaluate(`(function(){
      var el = document.querySelector('.rename-input');
      if (!el) return;
      el.focus();
      el.value = 'file-a.zip';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
    await main.evaluate(`(function(){
      var el = document.querySelector('.rename-input');
      if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`)
    return '[data-dialog-id="extension-change"]'
  })

  // Conflict-resolution dialog: the inline `.conflict-section` inside the
  // transfer-progress dialog. Stage a same-name collision (a `file-a.txt` in
  // `right/`), copy `file-a.txt` left→right under the "Ask for each" (stop)
  // policy so the per-file conflict prompt opens rather than an upfront policy.
  await mainOverlay('conflict-dialog', async () => {
    // Recreate first: an earlier surface (extension-change cancel) may leave the
    // tree perturbed, and we need `file-a.txt` present on both sides to collide.
    recreateFixtures(getFixtureRoot())
    writeFile(getFixtureRoot(), 'right/file-a.txt', 'dest-collision')
    await ensureAppReady(main)
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-a.txt')
    await dispatchMenuCommand(main, 'file.copy')
    await main.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(main)
    await clickTransferStart(main)
    await main.waitForSelector('[data-dialog-id="transfer-progress"]', 3000)
    return '.conflict-section'
  })
  // The conflict flow opens the transfer-progress modal; cancel it cleanly so it
  // doesn't run the copy or leak into later surfaces.
  await dismissOverlay(main).catch(() => {})

  // Go-to-path dialog (`nav.goToPath`).
  await mainOverlay('go-to-path', async () => {
    await dispatchMenuCommand(main, 'nav.goToPath')
    return '[data-dialog-id="go-to-path"] input[aria-label="Path to go to"]'
  })

  // Copy/move transfer dialog (F5 → `file.copy`): the source→dest picker with the
  // operation toggle and counters, BEFORE confirming. No collision here, so it
  // shows the plain confirm state (distinct from the conflict surface above).
  await mainOverlay('transfer-dialog', async () => {
    // Recreate first: the conflict surface left a collision in `right/`; a clean
    // tree gives the plain confirm state (no upfront conflict-policy block).
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(main)
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-b.txt')
    await dispatchMenuCommand(main, 'file.copy')
    return TRANSFER_DIALOG
  })

  // Command palette (`app.commandPalette`).
  await mainOverlay('command-palette', async () => {
    await dispatchMenuCommand(main, 'app.commandPalette')
    return '.palette-overlay .search-input'
  })

  // Search dialog (`search.open`). Shares the `.search-overlay` markup with the
  // selection dialog; captured FIRST so search-specific keys couple here and the
  // selection dialog below only claims its remaining unique keys.
  await mainOverlay('search-dialog', async () => {
    await dispatchMenuCommand(main, 'search.open')
    return '.search-overlay .query-input'
  })

  // Filter-chip popover: open the Search dialog, then the Size filter chip's
  // popover, for the chip/popover copy. The search dialog was torn down above,
  // so re-open it here.
  await mainOverlay('filter-popover', async () => {
    await dispatchMenuCommand(main, 'search.open')
    await main.waitForSelector('.search-overlay', 5000)
    await main.click('.search-overlay .chip-filter[aria-label="Size"]')
    return '.search-overlay .ui-popover'
  })
  // The popover sits ON the search dialog; dismiss both (popover first).
  await dismissOverlay(main).catch(() => {})

  // Selection dialog (`selection.selectFiles`): the "Select files…" twin of the
  // search dialog (same `QueryDialog` markup), so most keys already coupled to
  // `search-dialog`; this claims the selection-only ones.
  await mainOverlay('select-dialog', async () => {
    await dispatchMenuCommand(main, 'selection.selectFiles')
    return '.search-overlay .query-input'
  })

  // Connect-to-server dialog: reachable from the Network volume's browser via the
  // "+ Connect to server…" pseudo-row. Switch the left pane to Network (MCP),
  // then double-click the connect row (a single click only moves the cursor onto
  // it; `handleConnectRowDoubleClick` is what opens the dialog).
  await mainOverlay('connect-to-server', async () => {
    await mcpSelectVolume('left', 'Network')
    await main.waitForSelector('.network-browser .connect-row', 10000)
    await main.evaluate(`(function(){
      var el = document.querySelector('.network-browser .connect-row');
      if (el) el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    })()`)
    return '[data-dialog-id="connect-to-server"]'
  })
  // Leave the panes back on local so nothing downstream inherits Network.
  await mcpSelectVolume('left', LOCAL_VOLUME_NAME).catch(() => {})
}

test.describe('i18n screenshot capture', () => {
  // Drives ~22 surfaces across several windows (main, dialogs, a separate
  // Settings window iterating 18 sections, the viewer, the shortcuts window),
  // with window open/close throughout — well over the 15s per-test default. As
  // surfaces grow each tranche, bump this. (A normal interaction test fits in
  // 15s; this is a multi-surface capture driver, not a normal test.)
  test('captures representative surfaces and writes the coupling report', async ({ tauriPage }) => {
    test.setTimeout(180000)
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

    // surface label → { keys, screenshot filename }, plus the surfaces that threw
    // unexpectedly (`failed`, hard error) and ones deliberately skipped as a
    // documented harness gap (`skipped`, not an error).
    const report: Record<string, SurfaceEntry> = {}
    const failed: string[] = []
    const skipped: string[] = []

    // Each surface goes through `captureSurface`, which isolates its failure so
    // one broken surface can't abort the whole run (the report is always written
    // below). The `stage` callback does the surface-specific setup and returns
    // the page to capture against; the helper runs the shared
    // setSurface → rerender → focus → settle → screenshot tail.

    // ── Surface 1: main dual-pane window ─────────────────────────────────────
    await captureSurface('main-window', report, failed, async () => {
      await ensureAppReady(main)
      await main.waitForSelector('.file-entry', 5000)
      await captureCall(main, 'reset')
      await captureCall<boolean>(main, 'enable')
      return { page: main }
    })

    // ── Surface 2: new-folder dialog (F7) ────────────────────────────────────
    // A modal overlay on the foreground main window, sharing the main sink. The
    // shared `settlePaint` in `captureSurface` ensures the just-opened modal is
    // in the composited frame the native capture reads.
    await captureSurface('new-folder-dialog', report, failed, async () => {
      await skipParentEntry(main)
      await main.keyboard.press('F7')
      await main.waitForSelector(MKDIR_DIALOG, 5000)
      await main.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
      return { page: main }
    })
    await dismissOverlay(main)
    await captureCall(main, 'disable')

    // ── Surface 3 + 4..N: Settings window (every section) ─────────────────────
    await captureSettingsWindow(main, report, failed)

    // ── Surface: file viewer window ──────────────────────────────────────────
    // Its own restricted WebviewWindow (own webview context + sink). Opened on a
    // real fixture file via the production `open-file-viewer` event. Default
    // text-viewer chrome only (toolbar + status bar); media/search are later.
    let viewer: TauriPage | undefined
    let viewerLabel: string | undefined
    await captureSurface('viewer', report, failed, async () => {
      viewer = await openViewerWindow(main, viewerFixturePath())
      viewerLabel = viewer.targetWindow
      if (!viewerLabel) throw new Error('viewer page has no targetWindow label')
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 15000)
      await captureCall(viewer, 'reset')
      await captureCall<boolean>(viewer, 'enable')
      return { page: viewer, focusLabel: viewerLabel }
    })
    if (viewer && viewerLabel) await closeScopedWindow(main, viewer, viewerLabel).catch(() => {})

    // ── Surface: About dialog (main window overlay) ──────────────────────────
    // About is an in-app dialog rendered into the MAIN window (NOT a separate
    // WebviewWindow), so it captures against the main page's sink. Opened via the
    // `app.about` command. Re-enable + setSurface BEFORE opening so the dialog's
    // mount-time `t()` calls record under `about` too. Foreground window, so the
    // shared settlePaint is enough — no set_focus. Captured BEFORE the shortcuts
    // window so the (separate-window) shortcuts open/close can't perturb the main
    // window's sink between the dialog mount and its key dump.
    await captureSurface('about', report, failed, async () => {
      await captureCall(main, 'setSurface', 'about')
      await captureCall<boolean>(main, 'enable')
      await dispatchMenuCommand(main, 'app.about')
      await main.waitForSelector('[data-dialog-id="about"]', 5000)
      return { page: main }
    })
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})

    // ── Main-window overlay surfaces (dialogs, palette, query UI) ─────────────
    // Every dialog/palette/query surface rendered into the MAIN window, staged by
    // a keypress or registry command. Extracted to `captureMainOverlays` to keep
    // each surface isolated and the test body's complexity in check.
    await captureMainOverlays(main, report, failed)

    // ── Surface: keyboard shortcuts window (KNOWN-SKIPPED) ────────────────────
    // The standalone Keyboard shortcuts WebviewWindow (label `shortcuts`,
    // `/shortcuts` route, opened by `help.openShortcuts`) opens and is visible,
    // but the playwright plugin's `eval` channel never returns for THIS window:
    // even a trivial `1+1` times out, while the same eval works on `main`,
    // `settings`, and `viewer-*`. The plugin's per-window native screenshot also
    // grabs the main window's frame for the `shortcuts` label. So the
    // setSurface/rerender/dump capture flow (all eval-based) can't run here.
    // Reproduced in isolation (window opened first and alone). Root cause sits in
    // the tauri-playwright fork's per-window eval/capture for this specific
    // window, which is out of scope to change here.
    //
    // Cost of skipping is low: this window renders only the `shortcuts.list.*`
    // keys via `ShortcutsList` (noShortcut, addedTooltip, disabledTooltip) — 4
    // keys total — and nothing else couples them today, so they stay uncoupled.
    // It is SKIPPED (not failed): a clean run shouldn't go red on a documented
    // harness gap. Tracked in `capture-skipped.json` for coverage honesty.
    //
    // TODO(lead-verify): confirm the eval-channel hang on the `shortcuts` window
    // during your run (try `page.evaluate('1+1')` against it). If it's fixable in
    // the fork (per-window eval result routing) or by a window-config tweak,
    // re-enable this surface by moving 'shortcuts' back into a `captureSurface`
    // call. The staging that WOULD work once eval responds is preserved below,
    // behind a short probe so it never costs the run 30s.
    skipped.push('shortcuts')
    console.warn(
      `[i18n-capture] surface shortcuts SKIPPED: tauri-playwright eval channel unresponsive for the ` +
        `'shortcuts' window (trivial evals time out; main/settings/viewer are fine). Owns 4 shortcuts.list.* keys.`,
    )
    let shortcuts: TauriPage | undefined
    try {
      await dispatchMenuCommand(main, 'help.openShortcuts')
      shortcuts = await main.waitForWindow((w) => w.label === 'shortcuts', { timeout: 10000 })
      // Probe the eval channel with a 3s budget. If it ever starts responding
      // (fork fix), capture the surface for real; otherwise leave it skipped
      // without burning the default 30s eval timeout.
      const evalWorks = await Promise.race([
        // Swallow the eventual 30s reject of the hung eval so it doesn't surface
        // as an unhandled rejection after the 3s probe loses the race.
        shortcuts
          .evaluate<number>('1+1')
          .then((v) => v === 2)
          .catch(() => false),
        new Promise<boolean>((r) => setTimeout(() => r(false), 3000)),
      ])
      if (evalWorks) {
        await focusWindow(shortcuts, 'shortcuts')
        await shortcuts.waitForSelector('.shortcuts-scroll .row', 10000)
        await captureCall(shortcuts, 'reset')
        await captureCall<boolean>(shortcuts, 'enable')
        await captureCall(shortcuts, 'setSurface', 'shortcuts')
        await captureCall(shortcuts, 'rerender')
        await focusWindow(shortcuts, 'shortcuts')
        await settlePaint(shortcuts)
        await shortcuts.screenshot({ path: join(screenshotsDir, 'shortcuts.png') })
        report['shortcuts'] = { screenshot: 'shortcuts.png', keys: await keysFor(shortcuts, 'shortcuts') }
        skipped.splice(skipped.indexOf('shortcuts'), 1)
        console.log(`[i18n-capture] shortcuts: ${String(report['shortcuts'].keys.length)} keys → shortcuts.png (eval recovered)`)
      }
    } catch {
      // Open/probe failed; stays skipped (already recorded above).
    } finally {
      if (shortcuts) await closeScopedWindow(main, shortcuts, 'shortcuts').catch(() => {})
    }

    // Always write the report with whatever succeeded. The shape stays a flat
    // `surface → { screenshot, keys }` map because `couple-screenshots.js`
    // consumes it directly (`Object.values(report)`); the failed- and skipped-
    // surface lists go to SIBLING files (coverage honesty) so the coupler
    // contract is untouched. Empty/absent sibling files mean a clean run.
    writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n')
    writeFileSync(failedPath, JSON.stringify(failed, null, 2) + '\n')
    writeFileSync(skippedPath, JSON.stringify(skipped, null, 2) + '\n')
    console.log(
      `[i18n-capture] ${String(Object.keys(report).length)} surfaces captured, ` +
        `${String(failed.length)} failed, ${String(skipped.length)} skipped → report at ${reportPath}`,
    )
    if (failed.length > 0) console.warn(`[i18n-capture] FAILED surfaces: ${failed.join(', ')}`)
    if (skipped.length > 0) console.warn(`[i18n-capture] SKIPPED surfaces (documented gaps): ${skipped.join(', ')}`)

    // Fail the test (non-zero) only on UNEXPECTED failures — but only AFTER
    // writing the report and attempting every surface, so partial progress is
    // never lost. Documented skips don't fail the run (they're logged + tracked).
    expect(failed, `surfaces failed to capture: ${failed.join(', ')}`).toEqual([])
  })
})
