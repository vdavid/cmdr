/**
 * Shared core for the Cmdr Playwright E2E helpers.
 *
 * Holds the pieces every themed submodule (and the specs) lean on: platform
 * constants, selectors, the key-name mapping, broad DOM-query helpers, and the
 * `sleep` / `pollUntil` primitives. Submodules import FROM here; they never
 * import from each other through this file, so there are no import cycles.
 *
 * Key differences from WebDriverIO (carried over from the original helpers):
 * - No jsClick() workaround needed: tauriPage.click() works on all elements
 * - No pressSpaceKey() workaround: keyboard.press('Space') works directly
 * - No Backspace dispatchEvent hack: keyboard.press('Backspace') works
 * - evaluate() takes a string expression, not a function
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

/** Union type for tauriPage. Works in both Tauri and browser mode. */
export type PageLike = TauriPage | BrowserPageAdapter

// ── Selectors ────────────────────────────────────────────────────────────────

export const MKDIR_DIALOG = '[data-dialog-id="mkdir-confirmation"]'
export const NEW_FILE_DIALOG = '[data-dialog-id="new-file-confirmation"]'
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

// ── Input / event-dispatch helpers ──────────────────────────────────────────

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
 * Drives the native drag-and-drop drop entry programmatically by emitting the
 * `e2e-trigger-file-drop` Tauri event the app's E2E-gated listener forwards to
 * `ExplorerAPI.triggerFileDrop` → `dragDrop.handleFileDrop`. Real OS drag can't
 * be synthesized in Playwright, so this exercises OUR drop handling (the shared
 * destination guard, source-volume resolution, and the transfer dialog) without
 * a real drag gesture.
 *
 * `targetFolderPath` drops onto a specific folder row instead of the pane root;
 * `operation` ('copy' default, or 'move') mirrors what the modifier-resolved op
 * would be. The dialog opens (or an alert/toast surfaces) exactly as a real drop
 * would, so the caller asserts with the normal dialog/alert/toast helpers.
 *
 * `recordedIdentity` models an IN-APP self-drag (what `triggerSelfFileDrop`
 * wraps): the drop builds its transfer from the recorded source volume + the
 * paths the volume knows (volume-relative for MTP/SMB), exactly as a real
 * self-drag does — NOT by resolving the pasteboard `paths`. This is the only
 * shape that reproduces the live MTP/SMB self-drag failure class. Omit it for a
 * genuine EXTERNAL drop (local absolute paths through the resolver).
 */
export async function triggerFileDrop(
  tauriPage: PageLike,
  paths: string[],
  targetPane: 'left' | 'right',
  options: {
    targetFolderPath?: string
    operation?: 'copy' | 'move'
    recordedIdentity?: { sourceVolumeId: string; sourcePaths: string[] }
  } = {},
): Promise<void> {
  const payload = JSON.stringify({
    paths,
    targetPane,
    targetFolderPath: options.targetFolderPath,
    operation: options.operation,
    recordedIdentity: options.recordedIdentity,
  })
  await tauriPage.evaluate(`(function(){
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'e2e-trigger-file-drop', payload: ${payload} });
    })()`)
}

/**
 * Drives the native drag-and-drop entry for an IN-APP self-drag — the shape that
 * reproduces the live MTP/SMB failure class (a virtual volume's RELATIVE listing
 * path lands on the pasteboard and, after wry's drop round-trip, looks exactly
 * like a local absolute path). The drop builds its transfer from the recorded
 * `{ sourceVolumeId, sourcePaths }` (app state recorded at drag start), never by
 * resolving the pasteboard paths, exactly as a real self-drag does.
 *
 * `pasteboardPaths` are the lossy paths the OS would deliver (used only for
 * hit-testing in the real flow); the transfer uses `recordedIdentity.sourcePaths`.
 * They're usually the same volume-relative strings, matching reality.
 */
export async function triggerSelfFileDrop(
  tauriPage: PageLike,
  recordedIdentity: { sourceVolumeId: string; sourcePaths: string[] },
  targetPane: 'left' | 'right',
  options: { targetFolderPath?: string; operation?: 'copy' | 'move'; pasteboardPaths?: string[] } = {},
): Promise<void> {
  await triggerFileDrop(tauriPage, options.pasteboardPaths ?? recordedIdentity.sourcePaths, targetPane, {
    targetFolderPath: options.targetFolderPath,
    operation: options.operation,
    recordedIdentity,
  })
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

// ── Fixture helpers ─────────────────────────────────────────────────────────

/** Returns the fixture root path from the CMDR_E2E_START_PATH environment variable. */
export function getFixtureRoot(): string {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root) throw new Error('CMDR_E2E_START_PATH env var is not set')
  return root
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
