/**
 * E2E for the bug the backend scan-wait exists to close: you could not background
 * a transfer while it was still counting.
 *
 * Before, a confirmed transfer had no `operationId` until its `TransferDialog`
 * preview finished walking, so the progress dialog rendered neither Pause nor
 * Background, `handleQueue` bailed on the missing id, and unmounting the dialog
 * cancelled the scan outright. For a large tree that window is the whole opening
 * experience, not a corner case. Now the backend registers the operation at
 * confirm and its own task waits for the preview, so the row, its controls, and
 * the quit gate all exist from the first frame.
 *
 * ⚠️ This needs a harness affordance and would not otherwise get written. E2E
 * fixture trees are deliberately tiny and `data-scan-state` signals "counting
 * done", which is the opposite of what this test has to hold. The
 * `set_test_scan_preview_delay` command (feature-gated to `playwright-e2e`)
 * makes the scanning window deterministic rather than a race against a 40-file
 * fixture.
 *
 * Requires `--features playwright-e2e`.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  closeScopedWindow,
  ensureAppReady,
  getFixtureRoot,
  moveCursorToFile,
  pressKey,
  TRANSFER_DIALOG,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const QUEUE_LABEL = 'queue'
/** The progress dialog (NOT the destination picker). */
const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'

/** Long enough that the dialog is unmistakably still in its scanning phase when
 *  the test acts, short enough that the spec never sits on it: every assertion
 *  below polls, so a green run leaves well before this elapses. */
const SCAN_DELAY_MS = 4_000

const SOURCE = 'scan-bg-src'

test.setTimeout(90_000)

/** A small directory to copy. Size is irrelevant here — the delay, not the tree,
 *  is what holds the scan open. */
function makeSourceDir(fixtureRoot: string): void {
  const dir = path.join(fixtureRoot, 'left', SOURCE)
  fs.mkdirSync(dir, { recursive: true })
  for (let i = 0; i < 6; i++) {
    fs.writeFileSync(path.join(dir, `file-${String(i)}.txt`), 'x'.repeat(512))
  }
}

test.beforeEach(async ({ tauriPage }) => {
  const fixtureRoot = getFixtureRoot()
  recreateFixtures(fixtureRoot)
  makeSourceDir(fixtureRoot)
  await ensureAppReady(tauriPage)
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('set_test_scan_preview_delay', { ms: ${String(SCAN_DELAY_MS)} })`,
  )
})

test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

test.afterEach(async ({ tauriPage }) => {
  await tauriPage.evaluate(`(async function() {
    try { await window.__TAURI_INTERNALS__.invoke('set_test_scan_preview_delay', { ms: null }); } catch (e) {}
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
    for (var i = 0; i < 60; i++) {
      var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
      if (!remaining || remaining.length === 0) break;
      await new Promise(function(r) { setTimeout(r, 100); });
    }
  })()`)
})

test.describe('Backgrounding a transfer that is still counting', () => {
  test('sends a scanning copy to the queue window instead of losing it', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()

    await moveCursorToFile(main, SOURCE)
    await pressKey(main, 'F5')
    await main.waitForSelector(TRANSFER_DIALOG, 5000)
    // Confirm straight away: the preview is held at its starting line, so the
    // operation is registered while the walk has produced nothing.
    await pressKey(main, 'Enter')

    await main.waitForSelector(PROGRESS_DIALOG, 8000)

    // The dialog is in its scanning phase and the operation already has a name.
    await expect
      .poll(
        async () =>
          main.evaluate<number>(`(async function() {
            var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
            return ops.length;
          })()`),
        { timeout: 8000 },
      )
      .toBeGreaterThan(0)

    const backgroundButton = await main.evaluate<boolean>(`(function() {
      var dialog = document.querySelector(${JSON.stringify(PROGRESS_DIALOG)});
      if (!dialog) return false;
      var byLabel = function(label) { return dialog.querySelector('button[aria-label="' + label + '"]'); };
      return !!(byLabel('Keep this running in the background') || byLabel('Send to the operation queue'));
    })()`)
    expect(backgroundButton, 'a scanning transfer must offer Background').toBe(true)

    // The Pause control stays away: a scan-wait has nothing to park, and the
    // backend declines the flip.
    const pauseButton = await main.evaluate<boolean>(`(function() {
      var dialog = document.querySelector(${JSON.stringify(PROGRESS_DIALOG)});
      return !!(dialog && dialog.querySelector('button[aria-label="Pause this operation"]'));
    })()`)
    expect(pauseButton, 'nothing to pause while the operation is only counting').toBe(false)

    // Background it. The modal unmounts; the operation keeps going.
    await main.evaluate(`(function() {
      var dialog = document.querySelector(${JSON.stringify(PROGRESS_DIALOG)});
      var btn =
        dialog.querySelector('button[aria-label="Keep this running in the background"]') ||
        dialog.querySelector('button[aria-label="Send to the operation queue"]');
      btn.click();
    })()`)

    const queuePage = await main.waitForWindow((w) => w.label === QUEUE_LABEL, { timeout: 10_000 })

    // The row is there, and the copy is still alive rather than cancelled with
    // the dialog that started it.
    await expect
      .poll(async () => queuePage.evaluate<number>(`document.querySelectorAll('.queue-row').length`), {
        timeout: 10_000,
      })
      .toBeGreaterThan(0)

    // And it finishes on its own once the held preview releases: the operation
    // outlived the dialog that started it, which is the whole point.
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'right', SOURCE, 'file-0.txt')), { timeout: 30_000 })
      .toBe(true)

    await closeScopedWindow(main, queuePage, QUEUE_LABEL)
  })
})
