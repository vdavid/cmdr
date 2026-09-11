/**
 * Main-window file-explorer surface captures for the i18n screenshot-capture
 * driver (`i18n-capture.spec.ts`).
 *
 * A sibling of `i18n-capture-surfaces.ts`, split off purely for the file-length
 * budget. Holds the data-driven sweep of main-window file-explorer surfaces that
 * the earlier dialog/window tranches missed: states the dual-pane explorer
 * reaches WITHOUT a separate window, a backend event, or a debug build: a live
 * multi-file selection (the selection-summary status bar + its tooltip) and the
 * Shift fork of the function-key bar. Plus the one explorer state that DOES need an
 * E2E hook: the friendly-error pane (`captureErrorPaneExample`).
 *
 * Each renders into the MAIN window's own capture sink and is reactive mounted
 * markup, so the normal `captureSurface` rerender path records its keys. Coupling
 * order is set by `i18n-capture-main-pass.ts`; this module holds no orchestration
 * of its own.
 */

import { expect } from './fixtures.js'
import { ensureAppReady, getFixtureRoot } from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { selectAll } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import {
  type SurfaceEntry,
  captureCall,
  captureSurface,
  keysFor,
  recordStagedSurface,
  shoot,
  stressLayoutIfWorstCase,
} from './i18n-capture-helpers.js'
import { isStageOnly } from './i18n-capture-config.js'
import { scanForClipping } from './i18n-capture-frame.js'

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
 *   swaps to its Shift fork (New file, Rename, Delete permanently, and the empty
 *   F-key slots), recording `fileExplorer.functionKeyBar.{newFile*,permanently*,
 *   deletePermanently*,noShift*}` the default fork never shows.
 *
 * The Quick Look educational toast (`fileExplorer.quickLookHint.*`) is NOT here:
 * `captureQuickLookHint` in `i18n-capture-staged.ts` owns it.
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
    // Wait on the bar's own `data-row` marker, ❌ never on what a chip says: the
    // overflow pass pseudolocalizes every label, and the key text follows the
    // user's bindings and the platform's Shift glyph.
    await main.waitForSelector('.function-key-bar[data-row="shift"]', 5000)
    return { page: main }
  })
  // Release Shift so nothing downstream inherits the Shift fork.
  await main
    .evaluate(`document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift', bubbles: true }))`)
    .catch(() => {})
  await captureCall(main, 'disable').catch(() => {})

  // ── Pane volume chooser ─────────────────────────────────────────────────────
  // The dropdown behind a pane's volume breadcrumb. It's a pane-owned overlay, not
  // a registered soft dialog (`UNREGISTERED_OVERLAY_ENTRIES` says so), so neither
  // the dialog tranche nor the gallery pass reaches it, and it's the only place the
  // sidebar's group headings and the favorites empty-state render.
  await captureSurface('pane-volume-chooser', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'pane-volume-chooser')
    await captureCall<boolean>(main, 'enable')
    // Click the breadcrumb's volume name, the same target ⌥F1 activates.
    await main.evaluate(`(function(){
      var pane = document.querySelector('.file-pane.is-focused') || document.querySelector('.file-pane');
      var name = pane && pane.querySelector('.volume-breadcrumb .volume-name');
      if (!name) throw new Error('no volume breadcrumb in the focused pane');
      name.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    })()`)
    await main.waitForSelector('.volume-dropdown .category-label', 5000)
    return { page: main }
  })
  // Escape closes the dropdown; the poll keeps a slow close from bleeding into the
  // next surface's shot.
  await main
    .evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`)
    .catch(() => {})
  await expect
    .poll(async () => main.evaluate<number>(`document.querySelectorAll('.volume-dropdown').length`), { timeout: 3000 })
    .toBe(0)
    .catch(() => {})
  await captureCall(main, 'disable').catch(() => {})
}

/**
 * Captures ONE real friendly-error pane as the REPRESENTATIVE image for the whole
 * `errors.*` family (listing / write / provider / git). Every friendly error
 * shares this presentation (a bold title, an explanation paragraph, and a
 * suggestion), so a single honest capture, plus the coupler's representative
 * `@key.screenshotNote`, lets a translator load one image for the entire family.
 *
 * Like a toast, the error copy is SNAPSHOT-RESOLVED: `renderListingError` calls
 * `getMessage('errors.listing.<reason>.*')` once at navigation time and stores
 * plain strings on the FriendlyError props, so a later `rerender()` never
 * re-records them. The sink must be enabled BEFORE the error renders. Flow:
 * reset + setSurface + enable, THEN inject a real OS error (EACCES) and navigate
 * into a subdir so the backend listing fails and the pane renders, capturing the
 * `errors.listing.*` keys it resolves. We screenshot the real pane, then navigate
 * back so the next surface (and the afterEach leak guard) starts clean.
 *
 * Uses the `inject_listing_error` Tauri command (feature-gated behind
 * `playwright-e2e`, present in every E2E build): the same hook
 * `error-pane.spec.ts` uses. The injected error is single-shot, so the cleanup
 * navigation succeeds naturally.
 */
export async function captureErrorPaneExample(
  label: string,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  main: TauriPage,
): Promise<void> {
  const screenshot = `${label}.png`
  const fixtureRoot = getFixtureRoot()
  const subDirPath = `${fixtureRoot}/left/sub-dir`
  const leftPath = `${fixtureRoot}/left`
  const startedAt = Date.now()
  try {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', label)
    await captureCall<boolean>(main, 'enable')

    // Inject EACCES (errno 13 → a friendly "No permission" error) and navigate
    // into sub-dir in one atomic step (no wait between): a background listing
    // could otherwise consume the single-shot injected error first.
    await main.evaluate(
      `window.__TAURI_INTERNALS__.invoke('inject_listing_error', { volumeId: 'root', errorCode: 13 })`,
    )
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'mcp-nav-to-path',
      payload: { pane: 'left', path: ${JSON.stringify(subDirPath)} }
    })`)
    // The error pane appearing IS the readiness signal: the keys were resolved
    // (and recorded) during the listing the navigation kicked off.
    await main.waitForSelector('.error-pane', 5000)
    if (isStageOnly()) {
      recordStagedSurface(label, report, startedAt)
      return
    }
    // Worst-case pass: max zoom + min window so the error title/explanation/
    // suggestion fight the tightest pane. No-op outside the worst-case pass.
    await stressLayoutIfWorstCase(main, 'main')
    await shoot(main, 'main', screenshot)
    report[label] = { screenshot, keys: await keysFor(main, label) }
    await scanForClipping(main, label)
    console.log(`[i18n-capture] ${label}: ${String(report[label].keys.length)} keys → ${screenshot}`)
  } catch (err) {
    failed.push(label)
    console.warn(`[i18n-capture] surface ${label} FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    await captureCall(main, 'disable').catch(() => {})
    // Navigate back to a real directory so the pane leaves the error state.
    await main
      .evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'mcp-nav-to-path',
        payload: { pane: 'left', path: ${JSON.stringify(leftPath)} }
      })`)
      .catch(() => {})
    await main.waitForSelector('.file-entry', 5000).catch(() => {})
  }
}
