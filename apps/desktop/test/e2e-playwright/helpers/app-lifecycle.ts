/**
 * App-readiness helper for the Cmdr Playwright E2E tests.
 *
 * `ensureAppReady` is the per-test entry point: it routes to the explorer page,
 * resets both panes to the local volume + fixture directories, waits for the
 * fixture files, and lands keyboard focus inside `.dual-pane-explorer`. See the
 * suite's CLAUDE.md § "`ensureAppReady` focus contract".
 */

import { ensureMcpClient, mcpReadResource } from '../../e2e-shared/mcp-client.js'
import { type PageLike, LOCAL_VOLUME_NAME, getFixtureRoot, isStateClean, pollUntil } from './core.js'
import { navigateToRoute } from './navigation.js'

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
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)

  // Dismiss any overlays (AI notification, etc.)
  await tauriPage.evaluate(`(function() {
        var btn = document.querySelector('.ai-notification .ai-button.secondary');
        if (btn) btn.click();
    })()`)
  // allowed-bare-poll: AI notification may or may not be present; precautionary dismiss, not a required assertion
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.ai-notification')), 3000)

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
  // ~zero overhead — only ~5 ms for one MCP `cmdr://state` read.
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
        5000,
      )
      if (!volumeReset) {
        throw new Error(`ensureAppReady: both panes did not return to local volume '${LOCAL_VOLUME_NAME}' within 5s`)
      }
      // Previously: double-Escape + best-effort modal-overlay poll to clean up
      // a dialog leaked by the volume-touching spec. The global afterEach
      // safety net in fixtures.ts now catches and auto-cleans any leaks at
      // the point of leak, so this defensive cleanup is no longer needed.
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
    async () => {
      return tauriPage.evaluate<boolean>(`(function() {
                var pane = document.querySelectorAll('.file-pane')[0];
                if (!pane) return false;
                var expected = ${JSON.stringify(leftExpected)};
                return expected.every(function(name) {
                  return !!pane.querySelector('[data-filename="' + name + '"]');
                });
            })()`)
    },
    10000,
  )
  if (!filesFound) {
    const actual = await tauriPage.evaluate<string[]>(`(function() {
            var pane = document.querySelectorAll('.file-pane')[0];
            if (!pane) return [];
            return Array.from(pane.querySelectorAll('.file-entry')).map(function(e) {
                return e.getAttribute('data-filename') || '';
            });
        })()`)
    throw new Error(
      `ensureAppReady: expected files ${JSON.stringify(leftExpected)} not found in left pane after 10s. ` +
        `Actual entries: ${JSON.stringify(actual)}. Fixture directory may need recreateFixtures() in beforeEach.`,
    )
  }

  // Wait for the deterministic `data-app-ready` signal set at the end of
  // `+page.svelte`'s onMount (after the keydown listener and all MCP / dialog
  // listeners are wired). This is the GATE. Once it's true, onMount has
  // finished and the subsequent click+focus won't race against handler
  // attachment or focus theft from late-mounting components.
  await tauriPage.waitForFunction("document.querySelector('.dual-pane-explorer')?.dataset.appReady === 'true'", 10000)

  // Click on a file entry in the left pane to ensure focus, then focus the
  // explorer container so keyboard events reach the handler.
  await tauriPage.evaluate(`(function() {
        var entry = document.querySelectorAll('.file-pane')[0]?.querySelector('.file-entry');
        if (entry) entry.click();
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
  // 2. The two `mcp-nav-to-path` events above navigate BOTH panes, and a direct
  //    `source: 'mcp'` navigation SHIFTS the focused pane to whichever pane it
  //    navigated (`navigate.ts` L1, `shiftFocus`). We navigate left then RIGHT,
  //    and each pane's focus-shift fires on its async listing-complete, so the
  //    right pane's shift can land AFTER our `entry.click()` above — leaving the
  //    RIGHT (empty) pane focused. A prior test that ended on the right pane
  //    (e.g. Copy's `Tab`) makes this the default, not the exception. macOS
  //    timing usually lets the click win; Linux Xvfb + WebKitGTK consistently
  //    lets the late right-pane shift win, which silently pointed every
  //    cursor-driven op at the wrong pane.
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
    6000,
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
