/**
 * i18n screenshot-capture driver.
 *
 * NOT a pass/fail test: a harness that drives the real app to a set of surfaces
 * and, for each, records which catalog keys render there (via the runtime's
 * capture mode in `$lib/intl/messages.svelte.ts`) and saves a native screenshot.
 * The output is a single JSON map (surface → keys + screenshot file) that
 * `scripts/couple-screenshots.ts` turns into `@key.screenshot` couplings.
 *
 * Run it like any single spec (the app must already be running; see the suite's
 * DETAILS.md § "Running a single spec"), or via `pnpm i18n:shots`, which gets the
 * E2E binary, launches, runs only this spec, and tears the app down. It runs only
 * under its own `i18n-capture` shard kind (`playwright.config.ts`), so a full suite
 * run doesn't spend time taking screenshots. The lane still runs its STAGING, with
 * no camera: `i18n-capture-staging.spec.ts`.
 *
 * This file is the thin ORCHESTRATOR: the ordered surface groups live in
 * `i18n-capture-main-pass.ts` (shared with that staging spec, which is also where
 * the coupling order is explained), and the per-group capture functions in the
 * `i18n-capture-*.ts` modules beside it. This file only runs the passes and writes
 * the report.
 */

import { writeFileSync, readFileSync, existsSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { test, expect } from './fixtures.js'
import { dismissAllToasts } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, fitFindings } from './i18n-capture-helpers.js'
import {
  screenshotsDir,
  reportPath,
  failedPath,
  skippedPath,
  isOverflowPass,
  isWorstCasePass,
  overflowLocale,
} from './i18n-capture-config.js'
import { clipFindings } from './i18n-capture-frame.js'
import { MAIN_PASS_STEPS } from './i18n-capture-main-pass.js'
import { captureLicensePass, captureFdaOnboardingPass } from './i18n-capture-staged.js'

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
  const failedBefore = failed.length

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

  // Clear anything still on screen before the harness's leak guard runs, exactly
  // as the main pass does. These mock passes stage one or two surfaces and finish
  // in seconds, but the virtual MTP device announces itself on ITS own schedule,
  // so its connect toast can land mid-pass on a toast no surface here staged. The
  // guard then fails a pass whose every surface captured cleanly, which reads as a
  // capture failure and isn't one.
  await dismissAllToasts(main).catch(() => {})

  // The loaded list still carries every earlier pass's failures, and those passes
  // already failed on them. Judge this pass only by what it added, or one main-pass
  // failure fails every launch after it too.
  const failedHere = failed.slice(failedBefore)
  expect(failedHere, `surfaces failed to capture in pass ${pass}: ${failedHere.join(', ')}`).toEqual([])
}

/**
 * Logs what growing windows to fit their surfaces turned up.
 *
 * The zoom column also lands in `capture-report.json` (and from there in the
 * coverage report), because a translator has to know when an image shows smaller
 * text than a user sees. The `unreachable` column stays a log line: it's a
 * possible UI BUG (a box clipping content nothing can scroll into view), which
 * belongs in front of whoever ran the capture, not in a translator's artifact.
 */
function logFitFindings(): void {
  const entries = Object.entries(fitFindings).sort((a, b) => a[0].localeCompare(b[0]))
  if (entries.length === 0) return
  for (const [surface, fit] of entries) {
    const notes = [
      fit.grewBy > 0 ? `window grew ${String(fit.grewBy)}px` : '',
      fit.zoom !== 100 ? `captured at ${String(fit.zoom)}% zoom` : '',
      fit.residual > 0 ? `still ${String(fit.residual)}px short` : '',
      fit.unreachable.length > 0 ? `UNREACHABLE content in ${fit.unreachable.join(', ')}` : '',
    ].filter(Boolean)
    console.log(`[i18n-capture] fit ${surface}: ${notes.join('; ')}`)
  }
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
  const title = isWorstCasePass
    ? `# Pseudolocale WORST-CASE overflow report (${overflowLocale}, 150% zoom, min window size)`
    : `# Pseudolocale overflow report (${overflowLocale})`
  lines.push(title)
  lines.push('')
  const intro = isWorstCasePass
    ? 'Generated by `pnpm i18n:shots:overflow --worst-case`. Drove every surface in the deliberately-long ' +
      'pseudolocale AT MAX UI ZOOM (150%) WITH EACH WINDOW SHRUNK TO ITS MINIMUM ALLOWED SIZE (the ' +
      'maximal-overflow scenario), and ran a best-effort DOM scan for text its own box clips '
    : 'Generated by `pnpm i18n:shots:overflow`. Drove every surface in the deliberately-long ' +
      'pseudolocale and ran a best-effort DOM scan for text its own box clips '
  lines.push(
    intro +
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
  // Drives ~65 surfaces across several windows (main, dialogs, a separate
  // Settings window iterating 18 sections, the viewer, the shortcuts and queue
  // windows), with window open/close throughout, well over the 15s per-test
  // default. As the surface set grows, bump this. (A normal interaction test fits
  // in 15s; this is a multi-surface capture driver, not a normal test.)
  test('captures representative surfaces and writes the coupling report', async ({ tauriPage }) => {
    // ❗ 300s, NOT the 180s this used to be, and ❌ don't "optimize" it back. A
    // clean pass finishes in ~90s, so the headroom is entirely for a DISTURBED
    // run: someone using the computer mid-run backgrounds the window, and each
    // affected surface then spends up to 3 verified attempts, which overran 180s.
    // The cost of overrunning is not just a slow failure — on timeout Playwright
    // destroys the plugin socket, so every remaining surface dies with a confusing
    // `Not connected` that reads like an app crash, INSTEAD of the blank-frame
    // message written precisely to explain what happened. A too-tight timeout
    // replaces this run's own diagnostic with noise. The worst-case pass adds
    // per-surface zoom + resize + an extra reflow settle on top.
    test.setTimeout(isWorstCasePass ? 480000 : 300000)
    const main = tauriPage as TauriPage
    mkdirSync(screenshotsDir, { recursive: true })

    // The orchestrator (`scripts/i18n-capture.ts`) drives several launches: the
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

    // Every surface goes through an engine that isolates its failure, so one broken
    // surface can't abort the whole run (the report is always written below). The
    // groups and their coupling order live in `i18n-capture-main-pass.ts`, shared
    // with the stage-only run the E2E lane drives.
    for (const step of MAIN_PASS_STEPS) await step.run(main, { report, failed, skipped })

    // ── Documented skips deferred beyond the mock-staged surfaces ─────────────
    // These surfaces need backend state / events we can't fake from the frontend
    // here, so they're SKIPPED (not failed) and tracked for coverage honesty:
    //  - low-disk warning (`lowDiskSpace.*`): needs disk-pressure state from the
    //    backend space monitor.
    //  - AI-suggestion surfaces (`ai.*`): need the AI backend / a configured
    //    provider, and an emitted suggestion.
    //  - indexing rescan-notification toast (`indexing.rescan.*`): a separate
    //    snapshot toast needing a typed rescan event with a reason discriminator.
    //    (The aggregation/replay checklist states ARE captured now, via the dev
    //    gallery in `captureIndexingGallery` above.)
    //  - AI cloud connection / setup states (`ai.*` cloud, `cloudSetup.*`): need a
    //    real (or mocked) AI backend + configured provider; no frontend-stageable
    //    event reaches the connected/error cloud states here.
    //  - SMB / network browser + connect/reconnect surfaces (`fileExplorer.network.*`,
    //    `smbReconnect.*`): need the live SMB Docker stack (the `smb-e2e` Cargo
    //    feature in the build PLUS `smb-servers/start.sh e2e` containers, vendored
    //    `.compose/` files, a running Docker daemon, and credentialed connect). That
    //    stack is far more invasive to bring up from this capture harness than the
    //    other passes (a different feature build + external Docker lifecycle), so
    //    it's the documented lower-priority skip. The servers hub the Network row
    //    opens (`servers-hub`) IS captured: it needs no server.
    // (The download + MTP-connected toasts were here; they're now captured in the
    // main pass above via `captureDownloadToasts` / `captureMtpConnectedToast`.)
    for (const deferred of [
      'toast-low-disk',
      'ai-suggestion',
      'ai-cloud',
      'toast-index-rescan',
      'network-browser',
      'smb-reconnect',
    ]) {
      skipped.push(deferred)
    }
    console.warn(
      `[i18n-capture] ${String(6)} surfaces SKIPPED (need backend events, a configured provider, or the SMB Docker stack): ` +
        `low-disk toast, AI suggestion + cloud states, indexing rescan toast, ` +
        `and the SMB network browser + reconnect (needs live containers).`,
    )

    // ── Documented skips: surfaces needing backend state or a new prod hook ────
    // SKIPPED (not failed), tracked for coverage honesty:
    //  - viewer large-copy confirm/refuse dialogs (`viewer.copyDialog.*`): only
    //    appear for a text selection over ~10 MB (confirm) / ~100 MB (refuse);
    //    no fixture stages a selection that large deterministically. The gallery
    //    has rows for both, but they're viewer-hosted, and the gallery pass keeps
    //    to main-window dialogs so no shot shows a backdrop the dialog never has.
    //  - viewer reload toast (`viewer.reloadToast.*`): needs a file-changed event
    //    from the backend watcher while the viewer is open; the frontend can't
    //    fire it here.
    //  - the LICENSE DETAILS view (`license-details`): the LicenseKeyDialog's
    //    committed-license view reads `getLicenseInfo()` (the stored,
    //    signature-verified key), which `CMDR_MOCK_LICENSE` does NOT populate (the
    //    mock only drives `AppStatus`, not the stored `LicenseInfo`). Reaching it
    //    needs a real committed test key in the store, out of scope. The paid
    //    About (`about-commercial` / `about-perpetual`), the commercial-reminder
    //    modal, and the expiration modal ARE captured in their license passes (every
    //    E2E build honors the mock).
    // ❌ Don't add the crash-report dialog here. It only mounts from a boot-time
    // pending crash, so it looks unreachable, but the gallery pass opens it
    // through its registry row: no new hook in `(main)/+layout.svelte` needed.
    for (const docSkip of ['viewer-copy-confirm', 'viewer-copy-refuse', 'viewer-reload-toast', 'license-details']) {
      skipped.push(docSkip)
    }
    console.warn(
      `[i18n-capture] ${String(4)} surfaces SKIPPED (need backend state, a huge selection, or a committed key): ` +
        `viewer large-copy confirm/refuse (need a >10 MB selection), viewer reload toast (needs a watcher ` +
        `event), and the license-details view (needs a real committed key, not just the AppStatus mock).`,
    )

    // Always write the report with whatever succeeded. The shape stays a flat
    // `surface → { screenshot, keys }` map because `couple-screenshots.ts`
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
    logFitFindings()

    // Clear anything still on screen before the harness's leak guard runs. Surface
    // helpers clean up after themselves, but the virtual MTP device announces itself
    // on its own schedule, so its connect toast can arrive after the MTP surface is
    // done and strand the run on a toast no surface staged.
    await dismissAllToasts(main).catch(() => {})

    // Fail the test (non-zero) only on UNEXPECTED failures, but only AFTER
    // writing the report and attempting every surface, so partial progress is
    // never lost. Documented skips don't fail the run (they're logged + tracked).
    expect(failed, `surfaces failed to capture: ${failed.join(', ')}`).toEqual([])
  })
})
