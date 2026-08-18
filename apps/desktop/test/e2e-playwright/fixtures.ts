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
 * - before the test, the main window's OS title becomes "<base> (Running: <test>)"
 * - after it returns, "(FINISHED)" is appended, so you can glance at the dock /
 *   Cmd-Tab / Linux title bar to see which spec is in flight (or stuck) without
 *   tailing the log.
 *
 * ❗ The decoration and the leak guard hang off an AUTO FIXTURE, not off
 * `test.beforeEach` / `test.afterEach`. A hook declared at module scope HERE
 * attaches to the suite of whichever spec file happened to trigger this import
 * in the worker, so every OTHER file that worker runs goes unguarded — which is
 * how leaked toasts and dirty fixture trees stayed invisible for everything but
 * the first file each worker loaded. An auto fixture applies to every test that
 * imports this `test`, and its teardown runs AFTER the spec's own `afterEach`
 * hooks, so a spec's own cleanup still gets checked.
 */

import { createTauriTest } from '@srsholmes/tauri-playwright'
import type { TestInfo } from '@playwright/test'
import { describeFixtureTreeDiff, diffFixtureTree, restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'

// Each parallel E2E shard spawns its own Tauri instance bound to a distinct
// Unix socket. The Go check runner sets CMDR_PLAYWRIGHT_SOCKET per shard.
const socketPath = process.env.CMDR_PLAYWRIGHT_SOCKET ?? '/tmp/tauri-playwright.sock'

const { test: baseTest, expect } = createTauriTest({
  // No devUrl: in Tauri mode, the app is already running with its built
  // frontend. Setting devUrl would redirect the webview to a nonexistent
  // dev server. devUrl is only used in browser mode (not applicable here).
  devUrl: '',

  // Tauri mode config
  mcpSocket: socketPath,
})

export { expect }

/**
 * The bare `test` with NO auto fixture, for the marketing capture only.
 *
 * That shard runs with no fixture tree at all (it photographs the developer's real
 * folders, so `CMDR_E2E_START_PATH` is deliberately unset), which makes the leak
 * guard's fixture diff meaningless there — and its overlay check actively wrong,
 * since the search master is a picture of an open dialog. Every other spec imports
 * `test` below and keeps both guards.
 */
export const captureTest = baseTest

/**
 * Every test gets the window-title decoration and, on the way out, the leak
 * guard. `auto: true` is what makes that true for every spec file rather than
 * for one file per worker (see the file header).
 */
export const test = baseTest.extend<{ cmdrTestGuards: undefined }>({
  cmdrTestGuards: [
    async ({ tauriPage }, use, testInfo) => {
      await decorateTitle(tauriPage, testInfo, '')
      await use(undefined)
      await decorateTitle(tauriPage, testInfo, ' (FINISHED)')
      await failOnLeaks(tauriPage)
    },
    { auto: true },
  ],
})

// Captured once per worker on the first decoration so suffixes don't accumulate
// across tests. Each shard owns its own Tauri instance + its own worker process,
// so this lives correctly per-shard.
let baseTitle: string | null = null

type EvaluatablePage = {
  evaluate: {
    (js: string): Promise<unknown>
    <T>(js: string): Promise<T>
  }
}

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

async function decorateTitle(tauriPage: EvaluatablePage, testInfo: TestInfo, suffix: string): Promise<void> {
  try {
    if (baseTitle === null) baseTitle = await readMainTitle(tauriPage)
    await setMainTitle(tauriPage, `${baseTitle} (Running: ${formatTestName(testInfo)})${suffix}`)
  } catch {
    // Title decoration is purely for human eyeballs — never block a test on it.
  }
}

/** Fails the test that leaked UI artifacts or a dirty fixture tree, and cleans both up. */
async function failOnLeaks(tauriPage: EvaluatablePage): Promise<void> {
  // ONE post-test leak guard, two kinds of leak: UI artifacts left on screen,
  // and a shared fixture tree left dirty. Both cascade into the NEXT test's
  // beforeEach if unchecked, where the failure surfaces against the wrong test
  // and reads as a flake. Both probes run unconditionally, both auto-clean
  // AFTER the failure decision (so a leak never cascades even when this hook
  // fails), and their messages are reported together.
  const leakReports: string[] = []

  // Fixture-tree leak. `left/` + `right/` are shared by every spec on the
  // shard, and roughly half of them mutate the tree; `recreateFixtures` in
  // your own beforeEach protects you, not whoever runs after your last test.
  // A pure filesystem comparison needs no pane, no watcher, and no flush, so
  // it names the spec that dirtied the tree instead of the one that meets it.
  // ~0.3 ms on a clean tree (measured on an M3 Max, 2026-08-08).
  try {
    const fixtureRoot = process.env.CMDR_E2E_START_PATH
    const drift = fixtureRoot === undefined ? null : diffFixtureTree(fixtureRoot)
    if (drift && fixtureRoot !== undefined) {
      restoreFixtureTree(fixtureRoot)
      leakReports.push(
        `Test left the shared fixture tree dirty:\n${describeFixtureTreeDiff(drift)}\n` +
          `Restore what the test mutated (a \`test.afterEach\` calling \`restoreFixtureTree(getFixtureRoot())\`, ` +
          `or \`recreateFixtures\` when the spec already rebuilds the tree). The tree has been repaired for the ` +
          `next test. See apps/desktop/test/e2e-playwright/DETAILS.md § "The fixture-tree leak guard".`,
      )
    }
  } catch (err) {
    // The guard must never be the reason a run dies. A fixture root that
    // vanished mid-test is itself worth reporting, but not worth masking the
    // real failure with a stack trace.
    leakReports.push(`Fixture-tree leak guard could not read the tree: ${String(err)}`)
  }

  // Overlay + toast leak. Catches tests that opened a dialog, popover,
  // dropdown, or toast without dismissing it.
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
    // mask the original failure with a probe error. The fixture report, which
    // needs no app at all, still stands.
    leaked = null
  }

  if (leaked && leaked.length > 0) await reportAndCleanOverlayLeak(tauriPage, leaked, leakReports)

  if (leakReports.length > 0) throw new Error(leakReports.join('\n\n'))
}

/** Auto-cleans the leaked overlays and toasts, and records what leaked. */
async function reportAndCleanOverlayLeak(
  tauriPage: EvaluatablePage,
  leaked: string[],
  leakReports: string[],
): Promise<void> {
  // Auto-clean: dispatch Escape on each leaked overlay (target-phase fires
  // the overlay-bound handler in ModalDialog, bubble-phase fires
  // window-bound handlers elsewhere). Click each toast's close button.
  //
  // TWICE, with the round-trip in between letting the DOM settle: Escape in the query
  // dialogs is a two-step while a live run is going (`QueryDialog.resolveEscape`),
  // where the first press stops the run and the second closes the dialog. One left a
  // search dialog on screen, and since this guard is all that keeps a leak off the
  // next test, every later test on the shard died on the same overlay. The second
  // round finds nothing to dispatch at when the first press already closed things.
  for (let round = 0; round < 2; round++) {
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
      // Best-effort cleanup; the report below is the load-bearing signal.
      break
    }
  }

  leakReports.push(
    `Test left UI artifacts open: ${leaked.join(', ')}. ` +
      `Use dismissOverlay() to close dialogs/popovers/dropdowns, dismissAllToasts() to clear toasts ` +
      `(or click each toast's X). See apps/desktop/test/e2e-playwright/CLAUDE.md § "Closing overlays" ` +
      `for the full rule and the dispatch-on-overlay-not-document rationale.`,
  )

  const stuck = await breakTheCascade(tauriPage)
  if (stuck !== null) leakReports.push(stuck)
}

/**
 * Last resort when the Escape auto-clean above did NOT clear the overlay, so the
 * app is wedged rather than merely untidy.
 *
 * Escape only ever closes a dialog the app is willing to close, and a
 * transfer-progress dialog over a LIVE operation is not: it's the operation's
 * only UI, so it stays up until the operation ends. That's the shape that turns
 * one bad test into a dead shard, because the app is shared and the backend
 * refuses to start the next operation while a transfer dialog is in the way. One
 * such wedge produced 196 downstream failures and took a CI run from 5.8 to 33.1
 * minutes, every one of them reporting the same leaked `.modal-overlay` rather
 * than anything about the test it was attributed to.
 *
 * So when Escape has failed, cancel whatever the dialog is waiting on and try
 * once more. The culprit's own failure is already recorded either way; this only
 * decides whether the rest of the shard still means anything.
 *
 * Returns null once the screen is clear, or a message naming the surviving
 * overlays when even cancelling could not free the app: the one case where
 * every later failure on this shard is noise, and the report says so up front.
 */
async function breakTheCascade(tauriPage: EvaluatablePage): Promise<string | null> {
  const stillOpen = async (): Promise<string[]> => {
    try {
      return (
        (await tauriPage.evaluate<string[] | null>(`(function(){
                return ['.ui-popover', '.palette-overlay', '.search-overlay', '.modal-overlay', '.volume-dropdown']
                    .filter(function(s){ return document.querySelector(s) !== null; });
            })()`)) ?? []
      )
    } catch {
      return []
    }
  }

  if ((await stillOpen()).length === 0) return null

  try {
    // Mirrors `operation-queue.spec.ts`'s drain: cancellation is REQUESTED, not
    // finished, when the call returns, so wait for the lane to empty, and dismiss
    // retained failures inside the loop (a retained row never clears itself, and
    // an op can die while the loop is already spinning). Then Escape again: with
    // nothing left to run, the dialog now accepts it.
    await tauriPage.evaluate(`(async function(){
            try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
            try {
                var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
                var ids = ops.map(function(o) { return o.operationId; });
                if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
            } catch (e) {}
            for (var i = 0; i < 50; i++) {
                try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
                var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
                if (!remaining || remaining.length === 0) break;
                await new Promise(function(r) { setTimeout(r, 100); });
            }
            for (var round = 0; round < 2; round++) {
                ['.ui-popover', '.palette-overlay', '.search-overlay', '.modal-overlay', '.volume-dropdown']
                    .forEach(function(s){
                        var el = document.querySelector(s);
                        if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
                    });
                await new Promise(function(r) { setTimeout(r, 100); });
            }
        })()`)
  } catch {
    // Fall through to the probe: what's on screen decides the verdict, not
    // whether the recovery call itself came back cleanly.
  }

  const survivors = await stillOpen()
  if (survivors.length === 0) return null
  return (
    `The app is WEDGED: ${survivors.join(', ')} survived both the Escape auto-clean and a full ` +
    `operation cancel, so every later test on this shard will fail on this same overlay and none of ` +
    `those failures mean anything. Fix THIS test; ignore the cascade below it. ` +
    `See apps/desktop/test/e2e-playwright/DETAILS.md § "Breaking the cascade".`
  )
}
