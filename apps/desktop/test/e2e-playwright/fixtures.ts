/**
 * Playwright test fixtures for Cmdr E2E testing.
 *
 * Uses tauri-playwright in Tauri mode: the test runner communicates with
 * the real Tauri app via a Unix socket, and commands are injected directly
 * into the webview via `webview.eval()`. No WebDriver, no HTTP server.
 *
 * Fixture lifecycle:
 * - globalSetup: creates the fixture directory tree (~170 MB)
 * - beforeEach: recreates small text files (keeps bulk .dat files)
 * - globalTeardown: deletes the fixture directory
 *
 * Window-title decoration:
 * - `beforeEach` sets the main window's OS title to "<base> (Running: <test>)"
 * - `afterEach` updates it to "<base> (Running: <test>) (FINISHED)"
 *   so you can glance at the dock / Cmd-Tab / Linux title bar to see which
 *   spec is in flight (or stuck) without tailing the log.
 */

import { createTauriTest } from '@srsholmes/tauri-playwright'
import type { TestInfo } from '@playwright/test'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { flushFileWatcher, getFixtureRoot } from './helpers/core.js'

// Each parallel E2E shard spawns its own Tauri instance bound to a distinct
// Unix socket. The Go check runner sets CMDR_PLAYWRIGHT_SOCKET per shard.
const socketPath = process.env.CMDR_PLAYWRIGHT_SOCKET ?? '/tmp/tauri-playwright.sock'

export const { test, expect } = createTauriTest({
  // No devUrl: in Tauri mode, the app is already running with its built
  // frontend. Setting devUrl would redirect the webview to a nonexistent
  // dev server. devUrl is only used in browser mode (not applicable here).
  devUrl: '',

  // Tauri mode config
  mcpSocket: socketPath,
})

// Captured once per worker on the first beforeEach so suffixes don't accumulate
// across tests. Each shard owns its own Tauri instance + its own worker process,
// so this lives correctly per-shard.
let baseTitle: string | null = null

type EvaluatablePage = { evaluate: (js: string) => Promise<unknown> }

/** Joins describe blocks + test title into "Section > test name" style. */
function formatTestName(info: TestInfo): string {
  const parts = info.titlePath
  const fileIdx = parts.findIndex((p) => /\.spec\.[tj]s$/.test(p))
  const tail = fileIdx >= 0 ? parts.slice(fileIdx + 1) : [info.title]
  return tail.filter((p) => p.length > 0).join(' › ')
}

async function readMainTitle(tauriPage: EvaluatablePage): Promise<string> {
  const result = await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|title', { label: 'main' })`)
  return typeof result === 'string' ? result : ''
}

async function setMainTitle(tauriPage: EvaluatablePage, title: string): Promise<void> {
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('plugin:window|set_title', { label: 'main', value: ${JSON.stringify(title)} })`,
  )
}

test.beforeEach(async ({ tauriPage }, testInfo) => {
  // Restore the shared fixture tree before EVERY test, so no spec can inherit
  // the tree a mutating one left behind.
  //
  // Doing this per-spec never held: `recreateFixtures` in your own `beforeEach`
  // protects YOU, not whoever runs after your last test. Roughly half the specs
  // didn't call it at all, so any of them landing behind a conflict / file-op
  // spec met that spec's tree and failed inside `ensureAppReady` with "expected
  // files not found" — a failure that names the victim, not the culprit, and
  // whose membership shifts with shard order, which is exactly what reads as
  // flake. `dialog-inset.spec.ts` was one instance of the class, fixed spec by
  // spec; this fixes the class.
  //
  // Safe to do globally and cheap enough to be free (1-2 ms, measured
  // 2026-08-08 on an M3 Max): it only touches `left/` and `right/`, preserving
  // `left/bulk/`. Every suite that carries state across its own tests already
  // builds it OUTSIDE those two (`brief-cursor-fixtures/`,
  // `full-page-nav-fixtures/`, the viewer specs' own dirs), and says so at the
  // fixture. A spec needing a different tree still wins: this hook is
  // registered on the base test, so it runs BEFORE the spec file's own
  // `beforeEach`.
  recreateFixtures(getFixtureRoot())
  // Restoring the tree on disk isn't enough: the pane keeps showing the listing
  // it had until something re-reads it, and an external delete+recreate reaches
  // it only through FSEvents, which lags and can drop under load. So a spec that
  // inherited a mutated tree still opened on the STALE entries and failed inside
  // `ensureAppReady`. `flushFileWatcher` re-reads every active listing through
  // the Volume trait, so the restore lands before the first assertion rather than
  // whenever delivery gets around to it.
  await flushFileWatcher(tauriPage)

  try {
    if (baseTitle === null) baseTitle = await readMainTitle(tauriPage)
    await setMainTitle(tauriPage, `${baseTitle} (Running: ${formatTestName(testInfo)})`)
  } catch {
    // Title decoration is purely for human eyeballs — never block a test on it.
  }
})

test.afterEach(async ({ tauriPage }, testInfo) => {
  try {
    if (baseTitle === null) baseTitle = await readMainTitle(tauriPage)
    await setMainTitle(tauriPage, `${baseTitle} (Running: ${formatTestName(testInfo)}) (FINISHED)`)
  } catch {
    // See beforeEach.
  }

  // Overlay + toast leak guard. Catches tests that opened a dialog, popover,
  // dropdown, or toast without dismissing it. Without this hook, leaked UI
  // state cascades silently into the next test's beforeEach, where the
  // failure surfaces against the wrong test and looks like a flake.
  //
  // The probe runs unconditionally; if the test itself already failed,
  // Playwright bundles the probe's findings with the original failure.
  //
  // Auto-clean (Escape on each overlay, click each toast's close button)
  // runs AFTER the failure decision so the next test starts from a clean
  // slate even when this hook fails. Leaks don't cascade.
  // `tauriPage.evaluate<T>()`'s generic asserts the return type, but the call
  // actually resolves to null when the focused window was destroyed mid-test
  // (e.g. the production-binding Escape tests in viewer.spec.ts and
  // settings.spec.ts). Widen the generic to `string[] | null` so the `!leaked`
  // null-guard below stays legibly necessary instead of being stripped by
  // `no-unnecessary-condition`.
  let leaked: string[] | null
  try {
    leaked = await tauriPage.evaluate<string[] | null>(`(function(){
            var overlays = ['.ui-popover', '.palette-overlay', '.search-overlay', '.modal-overlay', '.volume-dropdown'];
            var found = overlays.filter(function(s){ return document.querySelector(s) !== null; });
            // Include each toast's first-100-char text in the leak label so
            // the failure message tells the test writer exactly what to assert
            // (e.g. \`expectAndDismissToast(tauriPage, 'Copy complete')\`).
            var toasts = document.querySelectorAll('.toast');
            for (var i = 0; i < toasts.length; i++) {
                var text = (toasts[i].textContent || '').replace(/\\s+/g, ' ').trim().slice(0, 100);
                found.push('.toast["' + text + '"]');
            }
            return found;
        })()`)
  } catch {
    // If the probe itself fails (e.g. the app crashed mid-test), don't
    // mask the original failure with a probe error.
    return
  }

  if (!leaked || leaked.length === 0) return

  // Auto-clean: dispatch Escape on each leaked overlay (target-phase fires
  // the overlay-bound handler in ModalDialog, bubble-phase fires
  // window-bound handlers elsewhere). Click each toast's close button.
  try {
    await tauriPage.evaluate(`(function(){
            var overlays = ['.ui-popover', '.palette-overlay', '.search-overlay', '.modal-overlay', '.volume-dropdown'];
            overlays.forEach(function(s){
                var el = document.querySelector(s);
                if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            });
            var btns = document.querySelectorAll('.toast .toast-close');
            for (var i = 0; i < btns.length; i++) btns[i].click();
        })()`)
  } catch {
    // Best-effort cleanup; the failure below is the load-bearing signal.
  }

  throw new Error(
    `Test left UI artifacts open: ${leaked.join(', ')}. ` +
      `Use dismissOverlay() to close dialogs/popovers/dropdowns, dismissAllToasts() to clear toasts ` +
      `(or click each toast's X). See apps/desktop/test/e2e-playwright/CLAUDE.md § "Closing overlays" ` +
      `for the full rule and the dispatch-on-overlay-not-document rationale.`,
  )
})
