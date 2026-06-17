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
 * key the FIRST surface (in this file's call order) it appeared on, so the most
 * specific / smallest surface that a key belongs to wins when ordered narrow-to-
 * broad below. Keep the surface order intentional.
 *
 * This file is the thin ORCHESTRATOR: the surface-driving helpers and the
 * per-group capture functions live in `i18n-capture-surfaces.ts`. Keep them there
 * (file-length budget); this file only sequences the surfaces and writes the
 * report.
 */

import { writeFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { test, expect } from './fixtures.js'
import {
  ensureAppReady,
  dismissOverlay,
  skipParentEntry,
  openViewerWindow,
  closeScopedWindow,
  dispatchMenuCommand,
  MKDIR_DIALOG,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import {
  type SurfaceEntry,
  screenshotsDir,
  reportPath,
  failedPath,
  skippedPath,
  viewerFixturePath,
  captureCall,
  keysFor,
  settlePaint,
  focusWindow,
  captureSurface,
} from './i18n-capture-helpers.js'
import {
  captureSettingsWindow,
  captureMainOverlays,
  captureFrontendToasts,
  captureEmptyPane,
  captureOnboardingWizard,
  captureWhatsNew,
  captureIndexingStatus,
} from './i18n-capture-surfaces.js'

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

    // ── Snapshot-resolved toast surfaces ──────────────────────────────────────
    // Command-handler confirmations + the transfer-complete toast. These resolve
    // their text ONCE at emit time, so the sink must be enabled BEFORE the action
    // fires (see `captureToastSurface`). Run after the dialogs so dialog keys
    // couple narrow-first.
    await captureFrontendToasts(main, report, failed)

    // ── Empty-directory pane messaging ────────────────────────────────────────
    await captureEmptyPane(main, report, failed)

    // ── Onboarding wizard (one surface per step) ──────────────────────────────
    await captureOnboardingWizard(main, report, failed)

    // ── What's-new post-update popup ──────────────────────────────────────────
    await captureWhatsNew(main, report, failed)

    // ── Drive-indexing status indicator ───────────────────────────────────────
    await captureIndexingStatus(main, report, failed)

    // ── Documented skips deferred to the mock-staging tranche ─────────────────
    // These surfaces need backend state / events we can't fake from the frontend
    // here, so they're SKIPPED (not failed) and tracked for coverage honesty.
    // They're the explicit charter of the next tranche (mock staging):
    //  - download toasts (`downloads.*`): need a real download-complete event
    //    from the updater / download manager, not a frontend command.
    //  - MTP-connected toast (`mtp.*`): needs a device or the `virtual-mtp`
    //    feature staged, absent from this capture build.
    //  - low-disk warning (`lowDiskSpace.*`): needs disk-pressure state from the
    //    backend space monitor.
    //  - AI-suggestion surfaces (`ai.*`): need the AI backend / a configured
    //    provider, and an emitted suggestion.
    //  - indexing rescan-notification toast (`indexing.rescan.*`): a separate
    //    snapshot toast needing a typed rescan event with a reason discriminator.
    //  - indexing aggregation/replay indicator states (`indexing.aggregation.*`,
    //    `indexing.replay.*`): need their own event pairs; `indexing-status`
    //    above covers the scan state only.
    for (const deferred of [
      'toast-download',
      'toast-mtp-connected',
      'toast-low-disk',
      'ai-suggestion',
      'toast-index-rescan',
      'indexing-aggregation',
      'indexing-replay',
    ]) {
      skipped.push(deferred)
    }
    console.warn(
      `[i18n-capture] ${String(7)} surfaces SKIPPED (deferred to the mock-staging tranche): ` +
        `download/MTP/low-disk toasts, AI suggestion, indexing rescan toast + aggregation/replay states. ` +
        `Each needs backend events or staged mocks the frontend can't fire here.`,
    )

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
        console.log(
          `[i18n-capture] shortcuts: ${String(report['shortcuts'].keys.length)} keys → shortcuts.png (eval recovered)`,
        )
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
