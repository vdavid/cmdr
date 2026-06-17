/**
 * Main-window file-explorer surface captures for the i18n screenshot-capture
 * driver (`i18n-capture.spec.ts`).
 *
 * A sibling of `i18n-capture-surfaces.ts`, split off purely for the file-length
 * budget. Holds the data-driven sweep of main-window file-explorer surfaces that
 * the earlier dialog/window tranches missed: states the dual-pane explorer
 * reaches WITHOUT a separate window, a backend event, or a debug build — a live
 * multi-file selection (the selection-summary status bar + its tooltip) and the
 * Shift fork of the function-key bar.
 *
 * Each renders into the MAIN window's own capture sink and is reactive mounted
 * markup, so the normal `captureSurface` rerender path records its keys. Coupling
 * order is set by the spec; this module holds no orchestration of its own.
 */

import { expect } from './fixtures.js'
import { ensureAppReady, getFixtureRoot } from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { selectAll } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface } from './i18n-capture-helpers.js'

/**
 * Clears any active selection in the focused pane via the production
 * `selection.deselectAll` command, so a leftover selection from one surface
 * doesn't bleed the selection-summary status bar into the next surface's shot.
 * Best-effort: a no-op when nothing is selected.
 */
async function clearSelection(main: TauriPage): Promise<void> {
  await main
    .evaluate(`(function(){
      var el = document.activeElement || document.body;
      // Cmd/Ctrl+Shift+A is the deselect-all binding (selection.deselectAll);
      // clicking a row instead would move the cursor, so dispatch the keydown the
      // centralized handler reads, mirroring the selectAll helper's shape.
      el.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true, shiftKey: true, metaKey: ${String(process.platform === 'darwin')}, ctrlKey: ${String(process.platform !== 'darwin')} }));
    })()`)
    .catch(() => {})
  await expect
    .poll(async () => main.evaluate<number>(`document.querySelectorAll('.is-selected').length`), { timeout: 2000 })
    .toBe(0)
}

/**
 * Captures the main-window file-explorer states not covered by the dialog and
 * window passes.
 *
 * - `selection-summary`: select every entry in the focused pane (Cmd/Ctrl+A) so
 *   `SelectionInfo` switches to its selection-summary mode, rendering
 *   `fileExplorer.summary.*` (the "N of M files/folders, P% selected" sentence)
 *   and, via the bar's `$derived` size tooltip, `fileExplorer.selectionTooltip.*`.
 *   The base `main-window` surface only ever sees the no-selection mode, so these
 *   keys had no home.
 * - `function-key-bar-shift`: hold Shift (dispatch a `Shift` keydown on the
 *   document, which the bar's `<svelte:document onkeydown>` reads) so the bar
 *   swaps to its Shift fork — New file, Rename, Delete permanently, and the empty
 *   F-key slots — recording `fileExplorer.functionKeyBar.{newFile*,permanently*,
 *   deletePermanently*,noShift*}` the default fork never shows.
 *
 * The Quick Look educational toast (`fileExplorer.quickLookHint.*`) is NOT here:
 * it's a documented skip in the spec. Its trigger (Space in the file list) gates
 * on the `fileExplorer.suppressQuickLookHint` setting, and the capture binary
 * reads the REAL prod tauri-store (the orchestrator launches it without a
 * `CMDR_DATA_DIR` override), where the setting is `true`, so the toast never
 * shows. See the spec's skip block for the full reason.
 *
 * Order is narrow-to-broad within the explorer: selection-summary first (its keys
 * are the most specific), then the Shift bar.
 */
export async function captureMainExplorerSurfaces(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  // A fresh tree (left/ has two files + a sub-dir) gives a deterministic
  // selection-summary count, and undoes any mutation an earlier surface left.
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)

  // ── selection-summary status bar ────────────────────────────────────────────
  await captureSurface('selection-summary', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'selection-summary')
    await captureCall<boolean>(main, 'enable')
    // Cmd/Ctrl+A selects every real entry; `SelectionInfo` flips to its
    // selection-summary mode once `selectedCount > 0` and `stats` populate.
    await selectAll(main)
    await main.waitForSelector('.selection-info .summary-text', 5000)
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})
  await clearSelection(main)

  // ── function-key bar, Shift fork ────────────────────────────────────────────
  await captureSurface('function-key-bar-shift', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'function-key-bar-shift')
    await captureCall<boolean>(main, 'enable')
    // The bar reads Shift via `<svelte:document onkeydown/onkeyup>`; a keydown
    // with `key:'Shift'` flips its `shiftHeld` rune and re-renders the Shift fork.
    // No keyup is dispatched, so it stays in the Shift state through the shot.
    await main.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift', bubbles: true }))`)
    // The Shift fork shows "Delete permanently" — wait for it before the rerender.
    await main.waitForSelector('.function-key-bar', 5000)
    await expect
      .poll(
        async () =>
          main.evaluate<boolean>(
            `Array.from(document.querySelectorAll('.function-key-bar button span')).some(function(s){ return s.textContent && s.textContent.toLowerCase().indexOf('perman') !== -1; })`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()
    return { page: main }
  })
  // Release Shift so nothing downstream inherits the Shift fork.
  await main
    .evaluate(`document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift', bubbles: true }))`)
    .catch(() => {})
  await captureCall(main, 'disable').catch(() => {})
}
