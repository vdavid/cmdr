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
            var overlays = ['.filter-chip-popover', '.palette-overlay', '.search-overlay', '.modal-overlay', '.volume-dropdown'];
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
            var overlays = ['.filter-chip-popover', '.palette-overlay', '.search-overlay', '.modal-overlay', '.volume-dropdown'];
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
