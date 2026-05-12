/**
 * Shared Playwright helpers for Cmdr E2E tests.
 *
 * These replace the WebDriverIO-based helpers from e2e-shared/helpers.ts,
 * using the TauriPage API instead of the `browser` global.
 *
 * Key differences from WebDriverIO:
 * - No jsClick() workaround needed — tauriPage.click() works on all elements
 * - No pressSpaceKey() workaround — keyboard.press('Space') works directly
 * - No Backspace dispatchEvent hack — keyboard.press('Backspace') works
 * - evaluate() takes a string expression, not a function
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { ensureMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'

/** Union type for tauriPage — works in both Tauri and browser mode. */
type PageLike = TauriPage | BrowserPageAdapter

// ── Selectors ────────────────────────────────────────────────────────────────

export const MKDIR_DIALOG = '[data-dialog-id="mkdir-confirmation"]'
export const TRANSFER_DIALOG = '[data-dialog-id="transfer-confirmation"]'

// ── Platform helpers ─────────────────────────────────────────────────────────

export const CTRL_OR_META = process.platform === 'darwin' ? 'Meta' : 'Control'

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
 * `tauriPage.keyboard.press()` — there's no Tauri-event equivalent.
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
  // This does NOT reset the directory — just ensures we're on the right route.
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
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)

  // Dismiss any overlays (AI notification, etc.)
  await tauriPage.evaluate(`(function() {
        var btn = document.querySelector('.ai-notification .ai-button.secondary');
        if (btn) btn.click();
    })()`)
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.ai-notification')), 3000)

  // Navigate both panes to the fixture root's left/ and right/ directories.
  // Previous tests may have entered sub-dir or navigated elsewhere.
  // Route navigation (above) only ensures we're on the explorer PAGE —
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
  const leftExpected = expectedFiles?.leftPane ?? ['file-a.txt', 'sub-dir']
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

  // Click on a file entry in the left pane to ensure focus, then focus the
  // explorer container so keyboard events reach the handler.
  await tauriPage.evaluate(`(function() {
        var entry = document.querySelectorAll('.file-pane')[0]?.querySelector('.file-entry');
        if (entry) entry.click();
        var explorer = document.querySelector('.dual-pane-explorer');
        if (explorer) explorer.focus();
    })()`)

  // Wait until a file entry has the cursor (focus confirmed)
  await tauriPage.waitForSelector('.file-pane .file-entry.is-under-cursor', 3000)

  // Confirm focus actually landed inside the explorer so keydown handlers
  // (both the container-level handler and the document-level shortcut dispatch
  // attached in +page.svelte's onMount) reach our F-keys. On back-to-back runs
  // the file-entry cursor can render before +page.svelte's onMount finishes
  // wiring `document.addEventListener('keydown', ...)`, leading to F5/F6/F8/Delete
  // being dropped. Poll for activeElement inside the explorer, then a tiny
  // margin to absorb the async listener attach.
  await tauriPage.waitForFunction(
    "document.activeElement && document.activeElement.closest('.dual-pane-explorer') !== null",
    3000,
  )
  await sleep(100)
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
    await pollUntil(
      tauriPage,
      async () => {
        const name = await tauriPage.evaluate<string>(`(function() {
                    var entry = document.querySelector('.file-entry.is-under-cursor');
                    if (!entry) return '';
                    return entry.getAttribute('data-filename') || '';
                })()`)
        return name !== '..'
      },
      3000,
    )
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

  // Confirm the cursor landed on the target file. Short timeout — `move_cursor`
  // is synchronous on the backend, this only covers the render tick.
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
    2000,
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
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.palette-overlay')), 3000)
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
 * throwing — when in doubt, the caller should do the full reset.
 */
export async function isStateClean(tauriPage: PageLike, localVolumeName: string): Promise<boolean> {
  try {
    const state = await mcpReadResource('cmdr://state')
    const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) =>
      line.replace(/^\n {2}volume: /, ''),
    )
    if (volumeLines.length < 2 || volumeLines[0] !== localVolumeName || volumeLines[1] !== localVolumeName) {
      return false
    }
    if (await tauriPage.isVisible('.modal-overlay')) return false
    return true
  } catch {
    return false
  }
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
  interval = 50,
): Promise<boolean> {
  const deadline = Date.now() + timeout
  while (Date.now() < deadline) {
    try {
      if (await condition()) return true
    } catch {
      // Element might not exist yet — keep polling
    }
    await sleep(interval)
  }
  return false
}
