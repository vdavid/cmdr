/**
 * App-readiness helper for the Cmdr Playwright E2E tests.
 *
 * `ensureAppReady` is the per-test entry point: it routes to the explorer page,
 * resets both panes to the local volume + fixture directories, waits for the
 * fixture files, and lands keyboard focus inside `.dual-pane-explorer`. See the
 * suite's CLAUDE.md § "`ensureAppReady` focus contract".
 */

import { waitBudget } from '../wait-budget.js'
import fs from 'fs'
import path from 'path'
import { expect } from '@playwright/test'
import { ensureMcpClient, mcpReadResource } from '../../e2e-shared/mcp-client.js'
import {
  type PageLike,
  LOCAL_VOLUME_NAME,
  TRANSFER_DIALOG,
  clickEntryInPane,
  flushFileWatcher,
  getFixtureRoot,
  isStateClean,
  pollUntil,
} from './core.js'
import { navigateToRoute } from './navigation.js'

/**
 * What the left pane must show once a spec has written its fixture: every
 * top-level entry of `left/`, read back from disk.
 *
 * `ensureAppReady`'s readiness poll is `expected.every(name => pane has it)`, so a
 * list NARROWER than the fixture passes on a partial listing. Fixture builders write
 * entries one at a time and the watcher coalescer can deliver a diff between two of
 * them, so a pane that latched a listing taken mid-write satisfies a few-name check
 * while missing the rest, and the spec fails later on a row that is on disk but not
 * on screen. Deriving the list from disk can't drift from what the spec wrote, which
 * a hand-maintained literal did.
 *
 * ❗ Call AFTER the fixture is written, never before: it reads the tree that exists
 * right now.
 */
export function expectedLeftPaneEntries(fixtureRoot: string): string[] {
  return fs
    .readdirSync(path.join(fixtureRoot, 'left'))
    .filter((name) => !name.startsWith('.'))
    .sort()
}

/**
 * The names from `expected` the LEFT pane's listing doesn't hold (`[]` once it holds
 * them all), or `null` while the pane has no listing to ask.
 *
 * Asks the backend listing cache by the pane's `data-listing-id`, ❌ never the DOM:
 * `FullList` / `BriefList` render only the virtual window's rows, so a
 * `[data-filename]` probe answers "is it drawn", and a wide expectation failed
 * whenever the window was too short to draw every name. A listing id the cache has
 * already dropped (the pane moved on mid-poll) reads as `null`, so the caller's poll
 * asks again with the fresh one.
 */
async function missingFromLeftListing(tauriPage: PageLike, expected: string[]): Promise<string[] | null> {
  return tauriPage.evaluate<string[] | null>(`(async function() {
    var pane = document.querySelectorAll('.file-pane')[0];
    var listingId = pane && pane.dataset.listingId;
    if (!listingId) return null;
    var expected = ${JSON.stringify(expected)};
    try {
      var found = await window.__TAURI_INTERNALS__.invoke('find_file_indices', {
        listingId: listingId, names: expected, includeHidden: true
      });
      return expected.filter(function(name) { return !(name in found); });
    } catch (e) {
      return null;
    }
  })()`)
}

/** Every row the left pane currently DRAWS, for a readiness failure's message only. */
async function drawnLeftRows(tauriPage: PageLike): Promise<string[]> {
  return tauriPage.evaluate<string[]>(`(function() {
    var pane = document.querySelectorAll('.file-pane')[0];
    if (!pane) return [];
    return Array.from(pane.querySelectorAll('.file-entry')).map(function(e) {
      return e.getAttribute('data-filename') || '';
    });
  })()`)
}

// ── App readiness ────────────────────────────────────────────────────────────

/**
 * Ensures the app is fully loaded and focus is initialized.
 * Waits for file entries, dismisses overlays, navigates the left pane back to
 * the fixture root's `left/` directory (in case a previous test changed it),
 * clicks a file entry, and focuses the explorer container.
 *
 * By default, waits for `['file-a.txt', 'sub-dir']` in the left pane.
 * Pass `expectedFiles` to wait for different files (useful after setting up
 * conflict fixtures with a different directory layout).
 */
export async function ensureAppReady(
  tauriPage: PageLike,
  expectedFiles?: { leftPane?: string[]; rightPane?: string[] },
): Promise<void> {
  // Navigate to the main route to ensure we're on the file explorer page.
  // This does NOT reset the directory. It just ensures we're on the right route.
  await navigateToRoute(tauriPage, '/')

  // Wait for file entries to be visible (confirms app is fully loaded)
  await tauriPage.waitForSelector('.file-entry', 15000)

  // Wait for the HTML loading screen to be gone
  await tauriPage.waitForFunction(
    '!document.querySelector("#loading-screen") || document.querySelector("#loading-screen").style.display === "none" || !document.querySelector("#loading-screen").offsetParent',
    5000,
  )

  // Close any lingering modal dialog from a prior test
  await tauriPage.evaluate(`(function() {
        var overlay = document.querySelector('.modal-overlay');
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    })()`)
  // allowed-bare-poll: modal may or may not be present from a prior test; precautionary dismiss, not a required assertion
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), waitBudget(3000))

  // Reset both panes back to the local volume if a previous test (smb,
  // mtp, mtp-conflicts, network-toggle) left one on Network/MTP/etc.
  // `mcp-nav-to-path` below is rejected by `DualPaneExplorer.navigateToPath`
  // for non-local panes, so the subsequent fixture-files poll would time out
  // with an empty pane. This is the same volume-reset the volume-touching
  // specs do in their own beforeEach — lifting it into `ensureAppReady`
  // means every spec gets it for free instead of needing to know about
  // the volume-pollution gotcha.
  //
  // Gated on `isStateClean` so the typical case (both panes already local,
  // no modal lingering) skips the volume-select + Escape sequence and pays
  // ~zero overhead: one DOM read of the pane breadcrumbs.
  try {
    await ensureMcpClient(tauriPage)
    if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
      await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} } });
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} } });
      })()`)
      // Wait for both panes to actually be on the local volume.
      const volumeReset = await pollUntil(
        tauriPage,
        async () => {
          const state = await mcpReadResource('cmdr://state')
          const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) =>
            line.replace(/^\n {2}volume: /, ''),
          )
          return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
        },
        waitBudget(5000),
      )
      if (!volumeReset) {
        throw new Error(`ensureAppReady: both panes did not return to local volume '${LOCAL_VOLUME_NAME}' within 5s`)
      }
      // No dialog cleanup here: `fixtures.ts`'s post-test leak guard catches and
      // auto-cleans an overlay at the point of leak, naming the spec that left it.
    }
  } catch {
    // mcp-client may not be available yet (very first test); fall through and
    // let the nav-to-path attempt run. If the pane is non-local the
    // expected-files poll below will fail with the existing clear error.
  }

  // Navigate both panes to the fixture root's left/ and right/ directories.
  // Previous tests may have entered sub-dir or navigated elsewhere.
  // Route navigation (above) only ensures we're on the explorer PAGE,
  // it doesn't change which directory the panes are showing.
  // We emit mcp-nav-to-path Tauri events which the +page.svelte listener
  // forwards to DualPaneExplorer.navigateToPath().
  const fixtureRoot = getFixtureRoot()
  const leftPanePath = fixtureRoot + '/left'
  const rightPanePath = fixtureRoot + '/right'
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'left', path: ${JSON.stringify(leftPanePath)} }
        });
        invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'right', path: ${JSON.stringify(rightPanePath)} }
        });
    })()`)

  // The leftExpected file poll below covers the wait for navigation to land.

  // Wait for the left pane to show the expected fixture files
  const leftExpected = expectedFiles?.leftPane ?? ['file-a.txt', 'file-b.txt', 'sub-dir']
  const filesFound = await pollUntil(
    tauriPage,
    async () => (await missingFromLeftListing(tauriPage, leftExpected))?.length === 0,
    waitBudget(10000),
  )
  if (!filesFound) {
    const missing = await missingFromLeftListing(tauriPage, leftExpected)
    throw new Error(
      `ensureAppReady: expected files ${JSON.stringify(leftExpected)} not in the left pane's listing after 10s. ` +
        `Missing: ${missing === null ? '(the pane has no listing)' : JSON.stringify(missing)}. ` +
        `Drawn rows: ${JSON.stringify(await drawnLeftRows(tauriPage))}. ` +
        `Fixture directory may need recreateFixtures() in beforeEach.`,
    )
  }

  // Drain the file-watcher backlog before returning. A prior spec's fixture
  // churn (recreateFixtures deletes+recreates `left/`) leaves debounced
  // remove/create diffs queued in the notify-debouncer; they land AFTER the
  // files-present poll above and either briefly empty the pane (breaking the
  // first cursor/selection read of the test) or, mid-drain, consume error-pane's
  // single-shot injected error. `flush_file_watcher` re-reads every active
  // listing now and flushes the coalescer, so the app is quiescent by return;
  // the re-confirm proves the left pane settled populated (not mid-reload).
  // Feature-gated to `playwright-e2e`, present in the E2E binary.
  await flushFileWatcher(tauriPage)
  const filesStable = await pollUntil(
    tauriPage,
    async () => (await missingFromLeftListing(tauriPage, leftExpected))?.length === 0,
    waitBudget(5000),
  )
  if (!filesStable) {
    const missing = await missingFromLeftListing(tauriPage, leftExpected)
    throw new Error(
      `ensureAppReady: the left pane's listing lost entries after flushing the file-watcher backlog ` +
        `(missing ${missing === null ? 'the whole listing' : JSON.stringify(missing)} of ${JSON.stringify(leftExpected)}). ` +
        `A background re-listing is still churning.`,
    )
  }

  // Wait for the deterministic `data-app-ready` signal set at the end of
  // `+page.svelte`'s onMount (after the keydown listener and all MCP / dialog
  // listeners are wired). This is the GATE. Once it's true, onMount has
  // finished and the subsequent click+focus won't race against handler
  // attachment or focus theft from late-mounting components.
  await tauriPage.waitForFunction("document.querySelector('.dual-pane-explorer')?.dataset.appReady === 'true'", 10000)

  // Click on a file entry in the left pane to ensure focus, then focus the
  // explorer container so keyboard events reach the handler. `clickEntryInPane`
  // waits for the row and throws if it never renders: a swallowed click here
  // used to surface as the cursor poll below timing out, which reads as "focus
  // is broken" rather than "the left pane was still empty".
  await clickEntryInPane(tauriPage, 0)
  await tauriPage.evaluate(`(function() {
        var explorer = document.querySelector('.dual-pane-explorer');
        if (explorer) explorer.focus();
    })()`)

  // Wait until a file entry has the cursor (focus confirmed). 6 s (not 8 s):
  // this and the focus-landed poll below stack inside `ensureAppReady` with no
  // early return, so 6000 + 6000 stays under the 15 s global timeout. An 8000
  // pair would let a stacked overrun trip the global-timeout abort, losing this
  // helper's specific error.
  await tauriPage.waitForSelector('.file-pane .file-entry.is-under-cursor', 6000)

  // Confirm focus landed inside the explorer AND the LEFT pane is the active
  // pane, so the container-level keydown handler reaches keys like Tab and
  // ArrowDown AND cursor-driven helpers (moveCursorToFile, F7-create, the
  // hidden-files toggle) read the left pane, which `ensureAppReady` just
  // navigated to `left/`. (Document-level F-key dispatch doesn't depend on
  // focus, but cursor-driven tests do.)
  //
  // Two effects fight for focus here, so we poll-and-recover instead of a
  // one-shot wait:
  //
  // 1. A late-mounting `ModalDialog` (CrashReportDialog from `+layout.svelte`,
  //    PtpcameradDialog, ExpirationModal, ...) calls `overlayElement?.focus()`
  //    on mount, stealing DOM focus from `.dual-pane-explorer`. The explorer's
  //    `onfocusin` guard can't reclaim it from an out-of-tree overlay.
  // 2. The two `mcp-nav-to-path` events above navigate BOTH panes, and the MCP
  //    nav listener shifts focus to whichever pane it navigated — synchronously
  //    when the nav is accepted (`mcp-listeners.ts` `setFocusedPane`; the
  //    commit-time `shiftFocus` in `navigate.ts` deliberately excludes `'mcp'`
  //    so there's no second, late-landing shift at listing-complete). The two
  //    listener-time shifts still run async relative to our fire-and-forget
  //    emits above, so the right-pane shift can land AFTER our `entry.click()`
  //    — leaving the RIGHT (empty) pane focused. A prior test that ended on the
  //    right pane (e.g. Copy's `Tab`) makes this the default, not the exception.
  //
  // On every iteration we dismiss any new modal overlay (Escape), re-request
  // left-pane focus by clicking the left `.file-pane` (its `onclick` →
  // `handleFocus('left')`), re-focus the explorer container, then check both
  // invariants. Re-clicking each pass outlasts the late right-pane focus-shift.
  const focusOk = await pollUntil(
    tauriPage,
    async () => {
      return tauriPage.evaluate<boolean>(`(function() {
                var overlay = document.querySelector('.modal-overlay');
                if (overlay) {
                    overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
                }
                var leftPane = document.querySelectorAll('.file-pane')[0];
                if (leftPane && !leftPane.classList.contains('is-focused')) {
                    // handlePaneClick -> onRequestFocus -> handleFocus('left').
                    leftPane.click();
                }
                var ae = document.activeElement;
                if (!ae || !ae.closest || ae.closest('.dual-pane-explorer') === null) {
                    var explorer = document.querySelector('.dual-pane-explorer');
                    if (explorer) explorer.focus();
                    ae = document.activeElement;
                }
                var focusInExplorer = !!(ae && ae.closest && ae.closest('.dual-pane-explorer') !== null);
                var leftFocused = !!(leftPane && leftPane.classList.contains('is-focused'));
                return focusInExplorer && leftFocused;
            })()`)
    },
    waitBudget(6000),
  )
  if (!focusOk) {
    const diag = await tauriPage.evaluate<string>(`(function() {
            var ae = document.activeElement;
            var panes = document.querySelectorAll('.file-pane');
            return JSON.stringify({
                active: ae ? { tag: ae.tagName, id: ae.id, cls: ae.className && ae.className.toString ? ae.className.toString() : '', isBody: ae === document.body } : 'null',
                focusedPaneIndex: Array.from(panes).findIndex(function(p){ return p.classList.contains('is-focused'); }),
                explorerExists: !!document.querySelector('.dual-pane-explorer'),
                appReady: document.querySelector('.dual-pane-explorer') ? document.querySelector('.dual-pane-explorer').dataset.appReady : 'no-explorer',
                overlays: Array.from(document.querySelectorAll('.modal-overlay, [role="dialog"], [role="alertdialog"]')).map(function(e){return {cls: e.className.toString(), id: e.id, dialogId: e.dataset && e.dataset.dialogId, visible: !!e.offsetParent};})
            });
        })()`)
    throw new Error(
      `ensureAppReady: focus did not land inside .dual-pane-explorer with the left pane active after 6s. State: ${diag}`,
    )
  }
}

/**
 * Waits until the BACKEND's operation registry is empty, so every operation the
 * test started has emitted its terminal event AND settled (its lane released).
 *
 * ❗ The name says `Backend` because that is the whole of what it proves. It asks
 * `list_operations` over IPC and nothing else, so there is no happens-before edge
 * between this returning and any FRONTEND state: the webview still has to take
 * `write-complete` off the event bridge, close the progress dialog, and raise the
 * toast (~0 ms idle, 650 ms measured in a loaded lane). A spec that waits here and
 * then asserts a dialog is gone is asserting on the other side of the boundary, and
 * it fails under load only. {@link waitForTransferUiToSettle} is that wait.
 *
 * The non-destructive sibling of {@link drainOperations}, which CANCELS what it
 * finds: this one waits for operations to finish on their own, and is what a
 * spec asserting on the OUTCOME of a write needs.
 *
 * ❗ Reach for this before asserting anything a write operation's COMPLETION
 * produces (its toast, its follow-up editor, the next operation starting
 * un-queued). The obvious waits do not stand in for it, because both can pass
 * while the operation is still running:
 *
 * - the copy is on disk well before the operation ends — every write lands by
 *   temp+rename and the closing `fdatasync` pass (`write_operations/durability.rs`,
 *   the user-visible "Writing the last piece…") runs AFTER the bytes are in
 *   place, and on a loaded Linux Docker box that flush has been measured taking
 *   seconds;
 * - the new row is in the pane before it ends too, because the pane gets it from
 *   its filesystem watcher rather than from the operation.
 *
 * So a spec that waits for the file and the row and then allows a toast the
 * three seconds a toast needs is really allowing the whole operation three
 * seconds, and it fails whenever the box is slow enough — with the progress
 * dialog still up and the toast, correctly, not yet raised.
 *
 * Waiting for the lane matters just as much for the operation AFTER this one: a
 * write reserves every lane it touches and the next one admits on settle, so a
 * second gesture fired between complete and settle is admitted QUEUED, and the
 * dialog hands it to the queue window instead of showing it (`handleAutoQueued`).
 * No toast is raised in this window at all, and the spec fails on an outcome it
 * never had.
 *
 * The default budget is deliberately far above any bound the app itself places
 * on winding down (the backend's 15 s `CANCEL_DRAIN_DEADLINE`, the frontend's
 * 20 s `CANCEL_SETTLE_FALLBACK_MS`), because it is not timing anything: a
 * settled registry ends the poll at once, so the number is only the point at
 * which "the operation never finished" is the honest verdict.
 *
 * ❌ Not a teardown helper: it never cancels and never dismisses a retained
 * failure, so a failed operation makes it time out and say so rather than
 * quietly tidying the evidence away. `drainOperations` is teardown's.
 */
export async function waitForBackendOperationsToSettle(
  tauriPage: PageLike,
  options: { timeout?: number } = {},
): Promise<void> {
  const timeout = options.timeout ?? waitBudget(30000)
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<number>(`(async function() {
            var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
            return ops ? ops.length : 0;
        })()`),
      { timeout },
    )
    .toBe(0)
}

/**
 * Every surface a transfer puts on screen: the setup dialog, the progress dialog it
 * hands over to, and the per-file conflict prompt the queue window raises.
 *
 * The conflict prompt nested INSIDE the progress dialog needs no entry of its own; it
 * unmounts with its host.
 */
const TRANSFER_UI_SELECTORS = [
  TRANSFER_DIALOG,
  '[data-dialog-id="transfer-progress"]',
  '[data-dialog-id="operation-conflict"]',
]

/** Which transfer surfaces are mounted right now, by `data-dialog-id`. */
async function openTransferSurfaces(tauriPage: PageLike): Promise<string[]> {
  const selectorsJson = JSON.stringify(TRANSFER_UI_SELECTORS)
  return (
    (await tauriPage.evaluate<string[] | null>(`(function() {
        return ${selectorsJson}
            .filter(function(s) { return document.querySelector(s) !== null; })
            .map(function(s) { return s.replace('[data-dialog-id="', '').replace('"]', ''); });
    })()`)) ?? []
  )
}

/**
 * Waits until a write is over on BOTH sides: the backend's registry has emptied and
 * the frontend has taken every transfer surface off the screen.
 *
 * ❗ Reach for this whenever the thing being asserted is a FRONTEND consequence of a
 * write finishing — no dialog on screen, the follow-up rename editor, the next
 * gesture reaching the app rather than a modal.
 * {@link waitForBackendOperationsToSettle} cannot stand in for it: the backend
 * finishing does not mean the UI has re-rendered, and there is no happens-before edge
 * between the two at all. On a fast machine the difference is invisible; under load
 * it fails, and it reads as flake rather than as the missing wait it is.
 *
 * It waits on the backend FIRST, deliberately. Polling for the dialogs alone would
 * pass vacuously against a gesture whose dialog hasn't opened yet, and a still-running
 * operation is a clearer verdict than "a dialog is up".
 *
 * ❌ Not a check that the screen is empty: it names the transfer surfaces and ignores
 * everything else, so a spec ending in the rename editor (what a single-item duplicate
 * opens) is settled, not leaking.
 */
export async function waitForTransferUiToSettle(
  tauriPage: PageLike,
  options: { backendTimeout?: number; uiTimeout?: number } = {},
): Promise<void> {
  await waitForBackendOperationsToSettle(tauriPage, { timeout: options.backendTimeout })
  await expect
    .poll(async () => openTransferSurfaces(tauriPage), { timeout: options.uiTimeout ?? waitBudget(10000) })
    .toEqual([])
}

/**
 * Cancels every operation still in flight and waits for the queue to empty.
 *
 * ❗ A mutating spec that may still be HOLDING an op must call this BEFORE
 * `restoreFixtureTree`, in ONE `afterEach` (Playwright runs same-suite `afterEach`s in
 * declaration order, so two hooks hide the order). A restore that runs under a live op
 * deletes that op's source out from under it, which costs a `Couldn't finish…` toast,
 * and the retained failure then poisons the NEXT test: it gets admitted behind the
 * queued op, sees "1 operation ahead of this one", and its own dialog never opens
 * inside the poll it allows.
 *
 * The dismiss sits inside the loop on purpose: an op can die while it is already
 * spinning, so one dismiss before the poll would miss it. Every call is best-effort —
 * a teardown that throws would mask the test's own verdict.
 */
export async function drainOperations(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(async function() {
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    for (var i = 0; i < 60; i++) {
      try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
      var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
      if (!remaining || remaining.length === 0) break;
      await new Promise(function(r) { setTimeout(r, 100); });
    }
  })()`)
}
