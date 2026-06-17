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

import { writeFileSync, readFileSync, existsSync, mkdirSync } from 'node:fs'
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
  captureErrorPaneExample,
  isOverflowPass,
  overflowLocale,
  clipFindings,
  scanForClipping,
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
import { captureMainDialogs, captureViewerSubsurfaces } from './i18n-capture-special.js'
import { captureMainExplorerSurfaces } from './i18n-capture-surfaces-main.js'
import {
  captureMtpSurfaces,
  captureDownloadToasts,
  captureQuickLookHint,
  captureLicensePass,
  captureFdaOnboardingPass,
} from './i18n-capture-staged.js'

/**
 * Runs ONE mock-staged pass (a non-`main` launch carrying a `CMDR_MOCK_LICENSE` /
 * `CMDR_MOCK_FDA` env). Loads the report + sibling failed/skipped lists the
 * `main` pass wrote, captures only this pass's surface(s), removes their labels
 * from `skipped` (they're being captured now), and writes everything back. So a
 * multi-launch run accumulates into one report instead of each launch clobbering
 * the last.
 *
 * Pass names: `license:commercial`, `license:perpetual`, `license:reminder`,
 * `license:expired` (each → `captureLicensePass`), and `fda:<variant>` (→
 * `captureFdaOnboardingPass`).
 */
async function runMockPass(pass: string, main: TauriPage): Promise<void> {
  const loadJson = <T>(p: string, fallback: T): T => {
    if (!existsSync(p)) return fallback
    try {
      return JSON.parse(readFileSync(p, 'utf8')) as T
    } catch {
      return fallback
    }
  }
  const report = loadJson<Record<string, SurfaceEntry>>(reportPath, {})
  const failed = loadJson<string[]>(failedPath, [])
  const skipped = loadJson<string[]>(skippedPath, [])

  const before = new Set(Object.keys(report))

  if (pass.startsWith('license:')) {
    await captureLicensePass(pass, main, report, failed)
  } else if (pass.startsWith('fda:')) {
    await captureFdaOnboardingPass(pass, main, report, failed)
  } else {
    throw new Error(`unknown capture pass: ${pass}`)
  }

  // Any newly-captured surface that was previously a documented skip leaves the
  // skip list (it's real now).
  for (const label of Object.keys(report)) {
    if (before.has(label)) continue
    const idx = skipped.indexOf(label)
    if (idx >= 0) skipped.splice(idx, 1)
  }

  writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n')
  writeFileSync(failedPath, JSON.stringify(failed, null, 2) + '\n')
  writeFileSync(skippedPath, JSON.stringify(skipped, null, 2) + '\n')
  console.log(
    `[i18n-capture] pass '${pass}': ${String(Object.keys(report).length)} surfaces in report, ` +
      `${String(failed.length)} failed, ${String(skipped.length)} skipped`,
  )
  expect(failed, `surfaces failed to capture in pass ${pass}: ${failed.join(', ')}`).toEqual([])
}

/**
 * Switches `page`'s app to the pseudolocale for the overflow pass (no-op for the
 * normal English coupling pass). The capture build exposes `setLocale` on the
 * capture API; the catalog for `overflowLocale` was baked into the glob at build
 * time (the orchestrator generates `en-XA` before building). Doing it via this
 * frontend-only seam is identical to the live Language picker, so the captured UI
 * is what a user switching language would see.
 */
async function switchToOverflowLocaleIfNeeded(page: TauriPage): Promise<void> {
  if (!isOverflowPass) return
  await captureCall(page, 'setLocale', overflowLocale)
  await settlePaint(page)
}

/**
 * Writes the human/agent-facing overflow clip report (overflow pass only). Lists,
 * per surface, the text-bearing elements the DOM scan found clipped in the
 * pseudolocale, so a reviewer goes straight to the N real tight spots instead of
 * eyeballing every screenshot. Best-effort heuristic (see `scanForClipping`):
 * absence here is not proof of a clean layout, and a flagged ellipsized label may
 * be acceptable design. Markdown so it reads in a diff/PR and stays small.
 */
function writeOverflowReport(): void {
  if (!isOverflowPass) return
  const entries = Object.entries(clipFindings).sort((a, b) => a[0].localeCompare(b[0]))
  const withClips = entries.filter(([, findings]) => findings.length > 0)
  const totalClips = withClips.reduce((n, [, findings]) => n + findings.length, 0)

  const lines: string[] = []
  lines.push(`# Pseudolocale overflow report (${overflowLocale})`)
  lines.push('')
  lines.push(
    'Generated by `pnpm i18n:overflow`. Drove every surface in the deliberately-long ' +
      'pseudolocale and ran a best-effort DOM scan for text its own box clips ' +
      '(`scrollWidth > clientWidth` / `scrollHeight > clientHeight` while `overflow` ' +
      'hides/ellipses it). This is a HEURISTIC: it can miss a clip an ancestor masks ' +
      'and can flag a deliberately-ellipsized label that is fine. Treat it as a list ' +
      'of spots to eyeball against the matching screenshot, not a pass/fail gate.',
  )
  lines.push('')
  lines.push(`Surfaces scanned: ${String(entries.length)}. Surfaces with clips: ${String(withClips.length)}. `)
  lines.push(`Clipped elements total: ${String(totalClips)}.`)
  lines.push('')
  if (withClips.length === 0) {
    lines.push(
      'No clipping found by the heuristic. Still skim the screenshots: a clip an ancestor masks reads clean here.',
    )
  } else {
    for (const [surface, findings] of withClips) {
      lines.push(`## ${surface} (${String(findings.length)})`)
      lines.push('')
      lines.push(`Screenshot: \`overflow/${surface}.png\``)
      lines.push('')
      for (const f of findings) {
        const dims = [
          f.overflowX > 0 ? `x +${String(Math.round(f.overflowX))}px` : '',
          f.overflowY > 0 ? `y +${String(Math.round(f.overflowY))}px` : '',
        ]
          .filter(Boolean)
          .join(', ')
        lines.push(`- \`${f.selector}\` (${dims}): ${f.text}`)
      }
      lines.push('')
    }
  }
  const out = join(screenshotsDir, 'overflow-report.md')
  writeFileSync(out, lines.join('\n') + '\n')
  console.log(
    `[i18n-overflow] clip report (${String(totalClips)} clips on ${String(withClips.length)} surfaces) → ${out}`,
  )
}

test.describe('i18n screenshot capture', () => {
  // Drives ~22 surfaces across several windows (main, dialogs, a separate
  // Settings window iterating 18 sections, the viewer, the shortcuts window),
  // with window open/close throughout, well over the 15s per-test default. As
  // the surface set grows, bump this. (A normal interaction test fits in
  // 15s; this is a multi-surface capture driver, not a normal test.)
  test('captures representative surfaces and writes the coupling report', async ({ tauriPage }) => {
    test.setTimeout(180000)
    const main = tauriPage as TauriPage
    mkdirSync(screenshotsDir, { recursive: true })

    // The orchestrator (`scripts/i18n-capture.js`) drives several launches: the
    // `main` pass (no mock) plus per-launch passes carrying a `CMDR_MOCK_LICENSE`
    // / `CMDR_MOCK_FDA` the app reads once at startup. `CMDR_I18N_CAPTURE_PASS`
    // names the active pass. The `main` pass writes the report fresh; every other
    // pass LOADS it, captures only its surface(s), and MERGES back, so a
    // multi-launch run accumulates into one report.
    const pass = process.env.CMDR_I18N_CAPTURE_PASS ?? 'main'
    if (pass !== 'main') {
      await runMockPass(pass, main)
      return
    }

    // The fixture auto-starts a video recorder (15 fps frame capture). It's
    // useless for this driver and just burns CPU + CoreGraphics work alongside
    // the screenshots, so stop it up front. Best-effort: never fail the run on it.
    try {
      await (main as unknown as { stopRecording: () => Promise<unknown> }).stopRecording()
    } catch {
      // Already stopped or unsupported; fine.
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
      // Overflow pass: switch the whole app to the pseudolocale BEFORE any surface
      // is captured, so every surface renders in the expanded, accented strings.
      await switchToOverflowLocaleIfNeeded(main)
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
    // `captureSurface` already isolated any staging failure (and recorded it in
    // `failed`). The cleanup must not itself throw out of the test when the dialog
    // never opened (e.g. a foreign window stole the F7 keypress): without the
    // `.catch`, `dismissOverlay`'s "no overlay is open" abort would skip every
    // later surface and the report write. Swallow it; the recorded failure still
    // fails the run at the end.
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})

    // ── Main-window file-explorer states (selection summary, Shift bar, hint) ──
    // Data-driven sweep of the dual-pane explorer states the dialog/window
    // tranches missed: a live multi-file selection (the selection-summary status
    // bar + its size tooltip), the Shift fork of the function-key bar, and the
    // Quick Look educational toast. All render into the main window's sink while
    // it's foreground, so they go here before the separate-window captures pull
    // focus away. Their keys are unique to these states, so coupling order is
    // immaterial; staged early to keep the main window the active surface.
    await captureMainExplorerSurfaces(main, report, failed)

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

    // ── Viewer subsurfaces (search, context menu, pickers, media) ─────────────
    // Each opens its own viewer window on a fixture file (text or media) and
    // captures a distinct viewer state. Run right after the base `viewer` surface
    // so viewer keys couple narrow (per-state) before any broader surface.
    await captureViewerSubsurfaces(main, report, failed, skipped)

    // ── Surface: About dialog (main window overlay) ──────────────────────────
    // About is an in-app dialog rendered into the MAIN window (NOT a separate
    // WebviewWindow), so it captures against the main page's sink. Opened via the
    // `app.about` command. Re-enable + setSurface BEFORE opening so the dialog's
    // mount-time `t()` calls record under `about` too. Foreground window, so the
    // shared settlePaint is enough, no set_focus. Captured BEFORE the shortcuts
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

    // ── Main-window report/feedback/license dialogs (default launch) ──────────
    // The license-key ENTRY dialog (Personal state), error-report, and feedback
    // dialogs, all main-window ModalDialogs opened by a registry command. The
    // commercial/expired license surfaces need a separate `CMDR_MOCK_LICENSE`
    // launch (the license pass below).
    await captureMainDialogs(main, report, failed)

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

    // ── Mock-staged MAIN-pass surfaces ────────────────────────────────────────
    // Reachable in the default launch now that the capture build carries
    // `virtual-mtp` + a hermetic default store + (debug-assertions) `CMDR_MOCK_FDA`:
    //  - Quick Look hint: fires on Space now that the default store doesn't
    //    suppress it (the data-dir fix).
    //  - MTP browse + connected toast: the virtual device auto-registers under
    //    E2E mode; the toast re-fires from the typed connect event.
    //  - Download teaching toast: emitted via the `download-detected` event with
    //    the FDA gate mocked open.
    // The per-launch license + FDA-variant surfaces run in their own passes (see
    // `runMockPass`), driven by the orchestrator's multi-launch loop.
    await captureQuickLookHint(main, report, failed)
    await captureMtpSurfaces(main, report, failed)
    await captureDownloadToasts(main, report, failed)

    // ── Representative friendly-error pane (the errors.* family) ───────────────
    // One real error pane captured in situ via the `inject_listing_error` E2E
    // hook. Every friendly error shares this title/explanation/suggestion layout,
    // so the coupler maps the whole uncoupled `errors.*` family to this image with
    // a `@key.screenshotNote` (see `REPRESENTATIVE_SCREENSHOTS` in
    // `scripts/couple-screenshots.js`). Run last among the main-pass surfaces so a
    // transient error state can't perturb earlier captures.
    await captureErrorPaneExample('error-message-example', report, failed, main)

    // ── Documented skips deferred beyond the mock-staged surfaces ─────────────
    // These surfaces need backend state / events we can't fake from the frontend
    // here, so they're SKIPPED (not failed) and tracked for coverage honesty:
    //  - low-disk warning (`lowDiskSpace.*`): needs disk-pressure state from the
    //    backend space monitor.
    //  - AI-suggestion surfaces (`ai.*`): need the AI backend / a configured
    //    provider, and an emitted suggestion.
    //  - indexing rescan-notification toast (`indexing.rescan.*`): a separate
    //    snapshot toast needing a typed rescan event with a reason discriminator.
    //  - indexing aggregation/replay indicator states (`indexing.aggregation.*`,
    //    `indexing.replay.*`): need their own event pairs; `indexing-status`
    //    above covers the scan state only.
    //  - AI cloud connection / setup states (`ai.*` cloud, `cloudSetup.*`): need a
    //    real (or mocked) AI backend + configured provider; no frontend-stageable
    //    event reaches the connected/error cloud states here.
    //  - SMB / network browser + connect/reconnect surfaces (`fileExplorer.network.*`,
    //    `smbReconnect.*`): need the live SMB Docker stack (the `smb-e2e` Cargo
    //    feature in the build PLUS `smb-servers/start.sh e2e` containers, vendored
    //    `.compose/` files, a running Docker daemon, and credentialed connect). That
    //    stack is far more invasive to bring up from this capture harness than the
    //    other passes (a different feature build + external Docker lifecycle), so
    //    it's the documented lower-priority skip. The
    //    connect-to-server DIALOG itself (`connect-to-server`) IS captured (reached
    //    from the empty Network volume, no server needed).
    // (The download + MTP-connected toasts were here; they're now captured in the
    // main pass above via `captureDownloadToasts` / `captureMtpSurfaces`.)
    for (const deferred of [
      'toast-low-disk',
      'ai-suggestion',
      'ai-cloud',
      'toast-index-rescan',
      'indexing-aggregation',
      'indexing-replay',
      'network-browser',
      'smb-reconnect',
    ]) {
      skipped.push(deferred)
    }
    console.warn(
      `[i18n-capture] ${String(8)} surfaces SKIPPED (need backend events, a configured provider, or the SMB Docker stack): ` +
        `low-disk toast, AI suggestion + cloud states, indexing rescan toast + aggregation/replay states, ` +
        `and the SMB network browser + reconnect (needs live containers).`,
    )

    // ── Documented skips: surfaces needing backend state or a new prod hook ────
    // SKIPPED (not failed), tracked for coverage honesty:
    //  - crash-report dialog (`crashReporter.*`): only mounts when boot's
    //    `checkPendingCrashReport()` IPC returns a pending crash; the gating +
    //    state live in `(main)/+layout.svelte` (runes-touching, `file-length`-
    //    flagged). There's no command or E2E event to force it, and adding an
    //    `e2e-show-crash-report` listener to that production file is out of scope
    //    for this capture work (a real prod-code change with its own review).
    //  - viewer large-copy confirm/refuse dialogs (`viewer.copyDialog.*`): only
    //    appear for a text selection over ~10 MB (confirm) / ~100 MB (refuse);
    //    no fixture stages a selection that large deterministically.
    //  - viewer reload toast (`viewer.reloadToast.*`): needs a file-changed event
    //    from the backend watcher while the viewer is open; the frontend can't
    //    fire it here.
    //  - the LICENSE DETAILS view (`license-details`): the LicenseKeyDialog's
    //    committed-license view reads `getLicenseInfo()` (the stored,
    //    signature-verified key), which `CMDR_MOCK_LICENSE` does NOT populate (the
    //    mock only drives `AppStatus`, not the stored `LicenseInfo`). Reaching it
    //    needs a real committed test key in the store, out of scope. The paid
    //    About (`about-commercial` / `about-perpetual`), the commercial-reminder
    //    modal, and the expiration modal ARE captured in their license passes (the
    //    debug-assertions capture build honors the mock).
    for (const docSkip of [
      'crash-report',
      'viewer-copy-confirm',
      'viewer-copy-refuse',
      'viewer-reload-toast',
      'license-details',
    ]) {
      skipped.push(docSkip)
    }
    console.warn(
      `[i18n-capture] ${String(5)} surfaces SKIPPED (need backend state, a new prod hook, or a committed key): ` +
        `crash-report dialog (boot-only pending-crash state), viewer large-copy confirm/refuse ` +
        `(need a >10 MB selection), viewer reload toast (needs a watcher event), and the license-details ` +
        `view (needs a real committed key, not just the AppStatus mock).`,
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
    // keys via `ShortcutsList` (noShortcut, addedTooltip, disabledTooltip), 4
    // keys total, and nothing else couples them today, so they stay uncoupled.
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
        new Promise<boolean>((r) =>
          setTimeout(() => {
            r(false)
          }, 3000),
        ),
      ])
      if (evalWorks) {
        await focusWindow(shortcuts, 'shortcuts')
        await shortcuts.waitForSelector('.shortcuts-scroll .row', 10000)
        if (isOverflowPass) await captureCall(shortcuts, 'setLocale', overflowLocale)
        await captureCall(shortcuts, 'reset')
        await captureCall<boolean>(shortcuts, 'enable')
        await captureCall(shortcuts, 'setSurface', 'shortcuts')
        await captureCall(shortcuts, 'rerender')
        await focusWindow(shortcuts, 'shortcuts')
        await settlePaint(shortcuts)
        await shortcuts.screenshot({ path: join(screenshotsDir, 'shortcuts.png') })
        report['shortcuts'] = { screenshot: 'shortcuts.png', keys: await keysFor(shortcuts, 'shortcuts') }
        await scanForClipping(shortcuts, 'shortcuts')
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
    writeOverflowReport()
    console.log(
      `[i18n-capture] ${String(Object.keys(report).length)} surfaces captured, ` +
        `${String(failed.length)} failed, ${String(skipped.length)} skipped → report at ${reportPath}`,
    )
    if (failed.length > 0) console.warn(`[i18n-capture] FAILED surfaces: ${failed.join(', ')}`)
    if (skipped.length > 0) console.warn(`[i18n-capture] SKIPPED surfaces (documented gaps): ${skipped.join(', ')}`)

    // Fail the test (non-zero) only on UNEXPECTED failures, but only AFTER
    // writing the report and attempting every surface, so partial progress is
    // never lost. Documented skips don't fail the run (they're logged + tracked).
    expect(failed, `surfaces failed to capture: ${failed.join(', ')}`).toEqual([])
  })
})
