/**
 * Shared Playwright helpers for Cmdr E2E tests.
 *
 * These replace the WebDriverIO-based helpers from e2e-shared/helpers.ts,
 * using the TauriPage API instead of the `browser` global.
 *
 * Key differences from WebDriverIO:
 * - No jsClick() workaround needed: tauriPage.click() works on all elements
 * - No pressSpaceKey() workaround: keyboard.press('Space') works directly
 * - No Backspace dispatchEvent hack: keyboard.press('Backspace') works
 * - evaluate() takes a string expression, not a function
 */

import { expect } from '@playwright/test'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { ensureMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'

/** Union type for tauriPage. Works in both Tauri and browser mode. */
export type PageLike = TauriPage | BrowserPageAdapter

/**
 * Overlay selectors that `dismissOverlay` and the global afterEach safety net know
 * about. Listed in priority order (foreground-most first): a popover open ON a
 * dialog should close before the dialog itself. The afterEach probes all of them
 * for leaks; `dismissOverlay` closes the topmost open one per call.
 *
 * If you add a new overlay surface (a new dialog kind, a new dropdown), add its
 * selector here so the safety net catches leaks of it too.
 */
const OVERLAY_SELECTORS = [
  '.filter-chip-popover',
  '.palette-overlay',
  '.search-overlay',
  '.modal-overlay',
  '.volume-dropdown',
] as const

// ── Selectors ────────────────────────────────────────────────────────────────

export const MKDIR_DIALOG = '[data-dialog-id="mkdir-confirmation"]'
export const TRANSFER_DIALOG = '[data-dialog-id="transfer-confirmation"]'

// ── Platform helpers ─────────────────────────────────────────────────────────

export const CTRL_OR_META = process.platform === 'darwin' ? 'Meta' : 'Control'

/**
 * Name of the local root volume in cmdr's volume picker. Linux Docker images
 * report it as "Root"; macOS uses "Macintosh HD". This must match the literal
 * `cmdr://state` volume entry for `mcp-volume-select` to pick the right one.
 */
export const LOCAL_VOLUME_NAME = process.platform === 'linux' ? 'Root' : 'Macintosh HD'

// ── Key name mapping ────────────────────────────────────────────────────────

/**
 * Maps Playwright key names to DOM `KeyboardEvent.key` values.
 * TauriKeyboard dispatches key names as-is, but the DOM spec uses
 * different values for some keys (for example, 'Space' -> ' ').
 */
const KEY_MAP: Record<string, string> = {
  Space: ' ',
  Backspace: 'Backspace',
  Enter: 'Enter',
  Escape: 'Escape',
  Tab: 'Tab',
}

/** Converts a Playwright key name to the DOM-compatible key value. */
export function mapKey(key: string): string {
  return KEY_MAP[key] ?? key
}

/**
 * Triggers a registry command directly via the `execute-command` Tauri event,
 * bypassing the keyboard-simulation path. Mimics what the OS native menu
 * accelerator does in prod (menu click → `on_menu_event` → `execute-command`).
 *
 * Use this for menu-bound shortcuts (F2/F7/F8, ⌘C/X/V, etc.) when the test
 * cares about the dialog/handler behavior rather than keyboard plumbing.
 * Synthesized DOM keystrokes don't trigger native menu accelerators and may
 * miss `handleGlobalKeyDown` if focus drifts after async MCP nav. The Tauri
 * event path is the direct equivalent and is unaffected by DOM focus state.
 *
 * For non-menu shortcuts (arrow keys, Space, Tab), keep using `pressKey()` /
 * `tauriPage.keyboard.press()`. There's no Tauri-event equivalent.
 *
 * @example
 * await dispatchMenuCommand(tauriPage, 'file.rename') // F2-equivalent
 * await dispatchMenuCommand(tauriPage, 'edit.copy')   // Cmd+C-equivalent
 */
export async function dispatchMenuCommand(tauriPage: PageLike, commandId: string): Promise<void> {
  const id = JSON.stringify(commandId)
  await tauriPage.evaluate(`(function(){
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'execute-command', payload: { commandId: ${id} } });
    })()`)
}

/**
 * Dispatches a keyboard event with the correct DOM key value.
 * Use this instead of tauriPage.keyboard.press() for keys that need mapping.
 */
export async function pressKey(tauriPage: PageLike, key: string): Promise<void> {
  const mapped = mapKey(key)
  const parts = mapped.split('+')
  const mainKey = parts[parts.length - 1]
  const modifiers = parts.slice(0, -1)
  const k = JSON.stringify(mainKey)
  const ctrl = modifiers.includes('Control') || false
  const shift = modifiers.includes('Shift') || false
  const alt = modifiers.includes('Alt') || false
  const meta = modifiers.includes('Meta') || false

  await tauriPage.evaluate(`(function(){
        var el=document.activeElement||document.body;
        var o={key:${k},bubbles:true,ctrlKey:${String(ctrl)},shiftKey:${String(shift)},altKey:${String(alt)},metaKey:${String(meta)}};
        el.dispatchEvent(new KeyboardEvent('keydown',o));
        el.dispatchEvent(new KeyboardEvent('keypress',o));
        el.dispatchEvent(new KeyboardEvent('keyup',o));
    })()`)
}

// ── Overlay + toast dismissal ───────────────────────────────────────────────

/**
 * Dismiss the topmost open overlay (modal dialog, command palette, search
 * dialog, filter-chip popover, volume picker dropdown) via synthetic Escape on
 * the overlay element itself, then assert it actually closed.
 *
 * Why dispatch on the overlay and not at the document or window level:
 *
 * - `ModalDialog.svelte` binds its `onkeydown` on the `.modal-overlay` div, not
 *   on `<svelte:window>`. A `document.dispatchEvent` bubbles up to `window` and
 *   never reaches the overlay's listener (events don't descend into subtrees).
 * - `tauriPage.keyboard.press('Escape')` works on macOS because the OS routes
 *   the keystroke to the focused element (the overlay focuses itself on
 *   mount), but flakes on Linux Xvfb where X11 focus delivery isn't reliable.
 * - Dispatching on the overlay element with `bubbles: true` reaches BOTH
 *   element-bound (target phase) and window-bound (bubble phase) listeners,
 *   so it's the universal pattern across overlay kinds.
 *
 * Throws if no overlay is open (catches tests that call dismiss when nothing
 * is up — typically a leak from an earlier step that already closed). Fails
 * via `expect.poll` if the overlay doesn't close within 3s.
 *
 * For toasts, use `dismissAllToasts` instead — toasts dismiss via a Close
 * button click, not via Escape.
 */
export async function dismissOverlay(tauriPage: PageLike): Promise<void> {
  const selectorsJson = JSON.stringify(OVERLAY_SELECTORS)
  const selector = await tauriPage.evaluate<string | null>(`(function(){
        var sels = ${selectorsJson};
        for (var i = 0; i < sels.length; i++) {
            if (document.querySelector(sels[i]) !== null) return sels[i];
        }
        return null;
    })()`)
  if (selector === null) {
    throw new Error(
      `dismissOverlay: no overlay is open (checked ${OVERLAY_SELECTORS.join(', ')}). ` +
        `If you expected one, something dismissed it earlier; ` +
        `if not, drop the dismissOverlay() call.`,
    )
  }
  const sel = JSON.stringify(selector)
  await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${sel});
        if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })()`)
  await expect.poll(async () => (await tauriPage.count(selector)) === 0, { timeout: 3000 }).toBeTruthy()
}

/**
 * Assert that exactly ONE toast whose text contains `substring` appears within
 * `timeout`, then dismiss that single toast.
 *
 * This is the ONLY public toast helper on purpose. Tests that trigger a
 * user-visible toast should care that it appeared (the toast IS the user-
 * facing confirmation; ship-the-wording is the contract) — not just clean
 * up after it. Bundling the assert + dismiss into one call removes the
 * "I'll just clean up the leak" shortcut: if you want to dismiss a toast,
 * you have to assert it first.
 *
 * Per-toast scoping (not blanket cleanup): the helper closes ONLY the first
 * `.toast` whose `.toast-message` text contains `substring`. Other toasts
 * stay open and will fail the test via the global afterEach safety net. If
 * your test fires multiple distinct toasts (rare), call this once per toast
 * with a substring that uniquely identifies each.
 *
 * Substring match is case-sensitive. Pass a stable prefix or unique fragment;
 * toast message format can change in non-load-bearing ways (whitespace,
 * pluralization) and assertion shouldn't break on those.
 *
 * The global afterEach safety net catches any toast you forgot to dismiss
 * and fails the test, so leaked toasts are loud, not silent.
 */
export async function expectAndDismissToast(
  tauriPage: PageLike,
  substring: string,
  options: { timeout?: number } = {},
): Promise<void> {
  const timeout = options.timeout ?? 3000
  const sub = JSON.stringify(substring)
  // Match against the WHOLE toast's textContent, not just `.toast-message`:
  // string-content toasts render their text in a `.toast-message` span, but
  // component-content toasts (`QuickLookHintToastContent`, the AI download
  // toast, error-report toasts) render the body straight into `.toast-content`
  // without that wrapper. Reading the toast element's textContent covers both.
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
            var toasts = document.querySelectorAll('.toast');
            for (var i = 0; i < toasts.length; i++) {
                if ((toasts[i].textContent || '').indexOf(${sub}) !== -1) return true;
            }
            return false;
        })()`),
      { timeout },
    )
    .toBeTruthy()
  // Click the close button on the SAME toast we just asserted, leaving any
  // other toasts open (they'll fail their own tests' afterEach checks).
  await tauriPage.evaluate(`(function(){
        var toasts = document.querySelectorAll('.toast');
        for (var i = 0; i < toasts.length; i++) {
            if ((toasts[i].textContent || '').indexOf(${sub}) !== -1) {
                var close = toasts[i].querySelector('.toast-close');
                if (close) close.click();
                return;
            }
        }
    })()`)
  // Wait for the specific toast to be gone. We poll the same substring match
  // to avoid races with neighboring toasts (which are out of scope here).
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
            var toasts = document.querySelectorAll('.toast');
            for (var i = 0; i < toasts.length; i++) {
                if ((toasts[i].textContent || '').indexOf(${sub}) !== -1) return false;
            }
            return true;
        })()`),
      { timeout: 2000 },
    )
    .toBeTruthy()
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

  // Confirm focus actually landed inside the explorer so the container-level
  // keydown handler reaches keys like Tab and ArrowDown. (Document-level F-key
  // dispatch doesn't depend on focus, but cursor-driven tests do.)
  //
  // Poll-and-recover instead of a one-shot waitForFunction: a late-mounting
  // ModalDialog (CrashReportDialog from `+layout.svelte`, PtpcameradDialog,
  // ExpirationModal, etc.) calls `overlayElement?.focus()` on mount, which
  // can steal focus from `.dual-pane-explorer` in the small window between
  // our `explorer.focus()` above and the assertion below. The explorer's
  // `onfocusin` guard cannot reclaim focus from an out-of-tree overlay.
  //
  // On every iteration we dismiss any new modal overlay (Escape), re-focus
  // the explorer, then check. Either focus already landed, or we recovered.
  const focusOk = await pollUntil(
    tauriPage,
    async () => {
      return tauriPage.evaluate<boolean>(`(function() {
                var overlay = document.querySelector('.modal-overlay');
                if (overlay) {
                    overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
                }
                var ae = document.activeElement;
                if (!ae || !ae.closest || ae.closest('.dual-pane-explorer') === null) {
                    var explorer = document.querySelector('.dual-pane-explorer');
                    if (explorer) explorer.focus();
                    ae = document.activeElement;
                }
                return !!(ae && ae.closest && ae.closest('.dual-pane-explorer') !== null);
            })()`)
    },
    6000,
  )
  if (!focusOk) {
    const diag = await tauriPage.evaluate<string>(`(function() {
            var ae = document.activeElement;
            if (!ae) return 'null';
            return JSON.stringify({
                tag: ae.tagName, id: ae.id,
                cls: ae.className && ae.className.toString ? ae.className.toString() : '',
                isBody: ae === document.body,
                explorerExists: !!document.querySelector('.dual-pane-explorer'),
                appReady: document.querySelector('.dual-pane-explorer') ? document.querySelector('.dual-pane-explorer').dataset.appReady : 'no-explorer',
                overlays: Array.from(document.querySelectorAll('.modal-overlay, [role="dialog"], [role="alertdialog"]')).map(function(e){return {cls: e.className.toString(), id: e.id, dialogId: e.dataset && e.dataset.dialogId, visible: !!e.offsetParent};})
            });
        })()`)
    throw new Error(`ensureAppReady: focus did not land inside .dual-pane-explorer after 6s. State: ${diag}`)
  }
}

// ── DOM query helpers ────────────────────────────────────────────────────────

/** Gets file entry name text from the cursor entry. Works with both view modes. */
export async function getEntryName(tauriPage: PageLike, selector: string): Promise<string> {
  return tauriPage.evaluate<string>(`(function() {
        var entry = document.querySelector('${selector}');
        if (!entry) return '';
        var colName = entry.querySelector('.col-name');
        if (colName) return colName.textContent || '';
        var name = entry.querySelector('.name');
        if (name) return name.textContent || '';
        return entry.textContent || '';
    })()`)
}

/** Checks whether a given filename exists in the focused pane's DOM listing. */
export async function fileExistsInFocusedPane(tauriPage: PageLike, targetName: string): Promise<boolean> {
  return tauriPage.evaluate<boolean>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return false;
        return !!pane.querySelector('[data-filename="${targetName}"]');
    })()`)
}

/** Checks whether a given filename exists in a specific pane (left=0, right=1). */
export async function fileExistsInPane(tauriPage: PageLike, targetName: string, paneIndex: number): Promise<boolean> {
  return tauriPage.evaluate<boolean>(`(function() {
        var panes = document.querySelectorAll('.file-pane');
        var pane = panes[${String(paneIndex)}];
        if (!pane) return false;
        return !!pane.querySelector('[data-filename="${targetName}"]');
    })()`)
}

/**
 * Finds the index of a file by name in the focused pane's entry list.
 * Returns the target index and total entry count, or an error object.
 */
export async function findFileIndex(
  tauriPage: PageLike,
  fileName: string,
): Promise<{ targetIndex: number; total: number } | { error: string }> {
  return tauriPage.evaluate<{ targetIndex: number; total: number } | { error: string }>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return { error: 'no focused pane' };
        var entries = pane.querySelectorAll('.file-entry');
        var targetIndex = -1;
        for (var i = 0; i < entries.length; i++) {
            if (entries[i].getAttribute('data-filename') === ${JSON.stringify(fileName)}) {
                targetIndex = i;
                break;
            }
        }
        return { targetIndex: targetIndex, total: entries.length };
    })()`)
}

// ── Cursor helpers ──────────────────────────────────────────────────────────

/** If the cursor is on the ".." parent entry, moves it down one position. */
export async function skipParentEntry(tauriPage: PageLike): Promise<void> {
  const cursorText = await tauriPage.evaluate<string>(`(function() {
        var entry = document.querySelector('.file-entry.is-under-cursor');
        if (!entry) return '';
        return entry.getAttribute('data-filename') || '';
    })()`)
  if (cursorText === '..') {
    await tauriPage.keyboard.press('ArrowDown')
    const moved = await pollUntil(
      tauriPage,
      async () => {
        const name = await tauriPage.evaluate<string>(`(function() {
                    var entry = document.querySelector('.file-entry.is-under-cursor');
                    if (!entry) return '';
                    return entry.getAttribute('data-filename') || '';
                })()`)
        return name !== '..'
      },
      8000,
    )
    if (!moved) {
      throw new Error('skipParentEntry: cursor did not leave the ".." entry within 8s after ArrowDown')
    }
  }
}

/**
 * Moves the cursor to a specific file by name in the focused pane.
 *
 * Uses the MCP `move_cursor` tool, which jumps directly to the target file
 * instead of pressing ArrowDown N times. Falls back to `false` if the file
 * isn't in the focused pane's listing (matching the prior behavior).
 *
 * The focused-pane detection reads `.file-pane.is-focused` from the DOM, so
 * the signature stays compatible with the old keyboard-based version. Tests
 * that explicitly exercise arrow-key cursor movement (`app.spec.ts`) keep
 * their own keyboard-driven helper and don't use this function.
 */
/**
 * Restores focus to `.dual-pane-explorer` (the keydown-handler host) before
 * an OS-keyboard test press. After MCP-driven actions like `move_cursor` the
 * focused element can drift to `<body>`, in which case `tauriPage.keyboard.press`
 * never reaches the explorer's handler. Mirrors the focus-recovery loop in
 * `ensureAppReady`, scoped to a single explicit call from a test mid-flow.
 *
 * Throws via `expect.poll` if focus can't be returned to the explorer in 2s
 * (an unexpected modal overlay or a missing explorer would do it).
 */
export async function ensureExplorerFocused(tauriPage: PageLike): Promise<void> {
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
          var ae = document.activeElement;
          if (!ae || !ae.closest || ae.closest('.dual-pane-explorer') === null) {
            var explorer = document.querySelector('.dual-pane-explorer');
            if (explorer) explorer.focus();
            ae = document.activeElement;
          }
          return !!(ae && ae.closest && ae.closest('.dual-pane-explorer') !== null);
        })()`),
      { timeout: 2000 },
    )
    .toBeTruthy()
}

/**
 * Focuses the requested pane (0 = left, 1 = right) so subsequent keyboard
 * input reaches the pane's handler. After MCP-driven actions like `move_cursor`,
 * `mcp-volume-select`, or `mcp-nav-to-path`, the DOM `.file-pane.is-focused`
 * marker can be absent (no pane is the "active" one), which means
 * `DualPaneExplorer.handleKeyDown`'s `getPaneRef(focusedPane)` returns undefined
 * and the keydown handler no-ops. We click the pane directly — the same gesture
 * a real user would use — to set focus deterministically, then poll the
 * `.is-focused` marker to confirm the click landed.
 *
 * Replaces the "toggle twice" idiom that depended on a specific starting
 * focus state (two `switch_pane` toggles only return to the originally-focused
 * pane; if no pane is focused, two toggles do nothing).
 */
export async function focusPane(tauriPage: PageLike, paneIndex: 0 | 1): Promise<void> {
  // Click the pane to set focus. Click coordinates come from the pane's bounding
  // rect (some inset to avoid hitting edge handles). The click is dispatched as
  // a synthetic MouseEvent so we don't depend on the OS pointer position.
  await tauriPage.evaluate(`(function(){
    var pane = document.querySelectorAll('.file-pane')[${String(paneIndex)}];
    if (!pane) throw new Error('pane ' + ${String(paneIndex)} + ' not found');
    var rect = pane.getBoundingClientRect();
    var x = rect.left + Math.min(40, rect.width / 2);
    var y = rect.top + Math.min(40, rect.height / 2);
    var opts = { bubbles: true, cancelable: true, view: window, clientX: x, clientY: y, button: 0 };
    pane.dispatchEvent(new MouseEvent('mousedown', opts));
    pane.dispatchEvent(new MouseEvent('mouseup', opts));
    pane.dispatchEvent(new MouseEvent('click', opts));
  })()`)
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(
          `document.querySelectorAll('.file-pane')[${String(paneIndex)}]?.classList.contains('is-focused') === true`,
        ),
      { timeout: 3000 },
    )
    .toBeTruthy()
}

export async function moveCursorToFile(tauriPage: PageLike, targetName: string): Promise<boolean> {
  // Bail early when the file isn't in the focused pane's listing. This matches
  // the previous behavior (returns false) so callers that assert `found === true`
  // still get the right signal.
  const info = await findFileIndex(tauriPage, targetName)
  if ('error' in info || info.targetIndex < 0) return false

  // Determine which pane is focused so we can target the right one via MCP.
  const focusedPane = await tauriPage.evaluate<'left' | 'right' | null>(`(function() {
        var panes = document.querySelectorAll('.file-pane');
        for (var i = 0; i < panes.length; i++) {
            if (panes[i].classList.contains('is-focused')) {
                return i === 0 ? 'left' : 'right';
            }
        }
        return null;
    })()`)
  const pane: 'left' | 'right' = focusedPane ?? 'left'

  await ensureMcpClient(tauriPage)
  await mcpCall('move_cursor', { pane, filename: targetName })

  // Confirm the cursor landed on the target file. `move_cursor` is synchronous on
  // the backend, so this only covers the render tick on a green run — it resolves
  // the moment the cursor lands and never reaches the budget. 8 s is failure
  // headroom for the shared Docker VM under load, where the render tick stretches.
  // Safe to keep at 8 s: this is the only bumped budget inside moveCursorToFile,
  // and the helper isn't called from ensureAppReady, so it never stacks.
  return pollUntil(
    tauriPage,
    async () => {
      return tauriPage.evaluate<boolean>(`(function() {
                var pane = document.querySelector('.file-pane.is-focused');
                if (!pane) return false;
                var entry = pane.querySelector('.file-entry.is-under-cursor');
                if (!entry) return false;
                return entry.getAttribute('data-filename') === ${JSON.stringify(targetName)};
            })()`)
    },
    8000,
  )
}

// ── Navigation helpers ──────────────────────────────────────────────────────

/**
 * Navigate to a SvelteKit route via link-click interception.
 * browser.url() doesn't work in Tauri, so we create a temporary `<a>` element
 * and click it to trigger SvelteKit's client-side routing.
 */
export async function navigateToRoute(tauriPage: PageLike, path: string): Promise<void> {
  await tauriPage.evaluate(`(function() {
        var a = document.createElement('a');
        a.href = ${JSON.stringify(path)};
        document.body.appendChild(a);
        a.click();
        a.remove();
    })()`)
}

// ── Multi-window helpers ────────────────────────────────────────────────────

/**
 * Opens a file viewer window via the production trigger and returns a TauriPage
 * scoped to the new viewer window.
 *
 * Uses the `open-file-viewer` Tauri event with a `{ path }` payload (the same
 * path the MCP server uses), wired in `routes/(main)/+page.svelte` to
 * `openFileViewer(path)` (creates a `viewer-<timestamp>` WebviewWindow). Then
 * polls `listWindows()` for a label starting with `viewer-`.
 *
 * @param filePath - File path to view. Pass an empty string to exercise the
 *   "missing path" error branch in `routes/viewer/+page.svelte`.
 */
export async function openViewerWindow(tauriPage: TauriPage, filePath: string): Promise<TauriPage> {
  const before = new Set((await tauriPage.listWindows()).map((w) => w.label).filter((l) => l.startsWith('viewer-')))
  const pathJson = JSON.stringify(filePath)
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'open-file-viewer', payload: { path: ${pathJson} } });
    })()`)
  // Wait for a NEW viewer-* window (not one left open from a previous test).
  const viewer = await tauriPage.waitForWindow((w) => w.label.startsWith('viewer-') && !before.has(w.label), {
    timeout: 10000,
  })
  return viewer
}

/**
 * Opens the settings window via the production trigger and returns a TauriPage
 * scoped to it. Uses the `open-settings` Tauri event, which `(main)/+page.svelte`
 * forwards to `openSettingsWindow()`. The settings window has the stable label
 * `settings`.
 */
export async function openSettingsWindowViaProd(tauriPage: TauriPage): Promise<TauriPage> {
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'open-settings' });
    })()`)
  return tauriPage.waitForWindow((w) => w.label === 'settings', { timeout: 10000 })
}

/**
 * Closes a scoped window (viewer or settings) and waits for it to disappear
 * from the window list. `mainPage` is needed for the post-close `listWindows()`
 * poll because the scoped page is gone once the window closes.
 *
 * Uses the Tauri window-close IPC directly instead of synthesizing Escape:
 * the viewer's Escape handler closes an open search bar first (one extra
 * Escape needed before the window-close path runs), and the settings window
 * may not have focus when afterEach kicks in. The window-close call has no
 * such gating and works regardless of in-page state.
 */
export async function closeScopedWindow(mainPage: TauriPage, scoped: TauriPage, label: string): Promise<void> {
  // Close the scoped window from the MAIN page, not the scoped page itself.
  // If we eval into the scoped window and call `plugin:window|close` there,
  // the window closes mid-script and never returns the pw_result IPC, so the
  // plugin times out waiting for the eval to finish (30 s) and blocks the
  // socket for the next test. Calling close from the main page is fire-and-
  // forget from the IPC plumbing's perspective. Main's response comes back
  // immediately, and the target window dies independently. (Touched arg
  // `scoped` is referenced to keep the API symmetrical with future helpers
  // that may need both pages.)
  void scoped
  const labelJson = JSON.stringify(label)
  try {
    await mainPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: ${labelJson} })`)
  } catch {
    // The window may already be gone; fall through to the poll.
  }
  const gone = await pollUntil(
    mainPage,
    async () => {
      const labels = (await mainPage.listWindows()).map((w) => w.label)
      return !labels.includes(label)
    },
    5000,
  )
  if (!gone) {
    throw new Error(`closeScopedWindow: window '${label}' still present after 5s`)
  }
}

// ── Fixture helpers ─────────────────────────────────────────────────────────

/** Returns the fixture root path from the CMDR_E2E_START_PATH environment variable. */
export function getFixtureRoot(): string {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root) throw new Error('CMDR_E2E_START_PATH env var is not set')
  return root
}

// ── Command palette ─────────────────────────────────────────────────────────

/**
 * Executes a command via the command palette. Opens the palette, types the
 * query, and clicks the first matching result.
 */
export async function executeViaCommandPalette(tauriPage: PageLike, query: string): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, shiftKey: true, bubbles: true
    }))`)
  await tauriPage.waitForSelector('.palette-overlay', 5000)
  await tauriPage.fill('.palette-overlay .search-input', query)
  // Wait for filtered results to appear
  await tauriPage.waitForSelector('.palette-overlay .result-item', 3000)
  await tauriPage.evaluate(`(function() {
        var item = document.querySelector('.palette-overlay .result-item');
        if (item) item.click();
    })()`)
  // Wait for palette to close after executing the command
  const paletteClosed = await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.palette-overlay')), 3000)
  if (!paletteClosed) {
    throw new Error('executeViaCommandPalette: palette did not close within 3s after clicking a result')
  }
}

// ── Size and count helpers ───────────────────────────────────────────────────

/** Gets the size column text for a named entry (Full view only). */
export async function getSizeText(tauriPage: PageLike, entryName: string, paneIndex = -1): Promise<string> {
  const paneSelector =
    paneIndex >= 0
      ? `document.querySelectorAll('.file-pane')[${String(paneIndex)}]`
      : `document.querySelector('.file-pane.is-focused')`
  const nameJson = JSON.stringify(entryName)
  return tauriPage.evaluate<string>(`(function() {
        var pane = ${paneSelector};
        if (!pane) return '';
        var entry = pane.querySelector('[data-filename=${nameJson}]');
        if (!entry) return '';
        var sizeEl = entry.querySelector('.col-size');
        return sizeEl ? sizeEl.textContent.trim() : '';
    })()`)
}

/** Counts file entries in a specific pane (0=left, 1=right). */
export async function countEntriesInPane(tauriPage: PageLike, paneIndex: number): Promise<number> {
  return tauriPage.evaluate<number>(`(function() {
        var pane = document.querySelectorAll('.file-pane')[${String(paneIndex)}];
        return pane ? pane.querySelectorAll('.file-entry').length : 0;
    })()`)
}

/** Counts entries whose name starts with a given prefix in the focused pane. */
export async function countEntriesWithPrefix(tauriPage: PageLike, prefix: string): Promise<number> {
  const prefixJson = JSON.stringify(prefix)
  return tauriPage.evaluate<number>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return 0;
        var entries = pane.querySelectorAll('.file-entry');
        var c = 0;
        for (var i = 0; i < entries.length; i++) {
            var name = entries[i].getAttribute('data-filename') || '';
            if (name.indexOf(${prefixJson}) === 0) c++;
        }
        return c;
    })()`)
}

// ── beforeEach state-cleanliness check ──────────────────────────────────────

/**
 * Returns true when the running app is in a "clean" pre-test state:
 *
 *   1. Both panes are on the named local volume (so subsequent
 *      `mcp-nav-to-path` events won't be rejected by a non-local pane).
 *   2. No modal-overlay element is visible in the DOM.
 *
 * Used by specs that touch volumes (mtp, mtp-conflicts, smb, network-toggle)
 * to short-circuit the per-test volume reset + Escape sequence when the
 * previous test already left things in a clean state. The full reset is
 * still needed when this returns false, and on the first test of each spec
 * (where a prior spec may have left a pane elsewhere).
 *
 * Reads `cmdr://state` over MCP. Caller must have already called
 * `initMcpClient(tauriPage)`. Returns false on any error rather than
 * throwing. When in doubt, the caller should do the full reset.
 */
export async function isStateClean(tauriPage: PageLike, localVolumeName: string): Promise<boolean> {
  try {
    // Combined DOM read: pane volume labels + modal-overlay presence in one
    // tauri-playwright evaluate. Skips the MCP `cmdr://state` HTTP roundtrip
    // (~30–50 ms per call), which used to dominate the beforeEach time on
    // every MTP test even though the DOM already had the answer.
    return await tauriPage.evaluate<boolean>(
      `(function(){
        var els = document.querySelectorAll('.volume-breadcrumb .volume-name');
        var name = ${JSON.stringify(localVolumeName)};
        if (els.length < 2) return false;
        for (var i = 0; i < 2; i++) {
          var t = (els[i].textContent || '').trim();
          if (t !== name) return false;
        }
        return !document.querySelector('.modal-overlay');
      })()`,
    )
  } catch {
    return false
  }
}

// ── E2E test-mode IPCs (feature-gated, not in typed bindings) ───────────────

/**
 * Forces the backend file watcher to flush any pending events.
 *
 * The debouncer + FSEvents/inotify add up to seconds of latency per FS
 * mutation under E2E. After this returns, every active watch has been
 * re-read and the frontend has received the corresponding `directory-diff`
 * event. See `commands/e2e.rs::flush_file_watcher` for the Rust side.
 *
 * Compiled only with the `playwright-e2e` Cargo feature; not in typed
 * bindings, so we call it via raw `__TAURI_INTERNALS__.invoke`.
 */
export async function flushFileWatcher(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('flush_file_watcher')`)
}

// ── Utility ─────────────────────────────────────────────────────────────────

export function sleep(ms: number): Promise<void> {
  if (process.env.SLEEP_LOG === '1') {
    const stack = new Error().stack ?? ''
    const lines = stack.split('\n')
    // index 0 = "Error", 1 = sleep itself, 2 = caller
    const frame = (lines[2] ?? '').trim().slice(0, 200)
    process.stdout.write(`[sleep] +${String(ms)}ms @ ${frame}\n`)
  }
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/**
 * Polls a condition function until it returns true or timeout is reached.
 * Similar to WebDriverIO's browser.waitUntil().
 */
export async function pollUntil(
  _page: PageLike,
  condition: () => Promise<boolean>,
  timeout: number,
  // 20 ms: the polled DOM/state reads are sub-millisecond, so the only cost of a
  // tighter interval is more cheap checks; the win is exiting ~one poll-tick after
  // the awaited event instead of overshooting by up to 50 ms. With 5-8 sequential
  // polls per conflict/mtp test, that overshoot was a real chunk of wall-clock.
  interval = 20,
): Promise<boolean> {
  const deadline = Date.now() + timeout
  while (Date.now() < deadline) {
    try {
      if (await condition()) return true
    } catch {
      // Element might not exist yet, keep polling
    }
    await sleep(interval)
  }
  return false
}
