/**
 * E2E tests for the native drag-and-drop ENTRY path, driven programmatically.
 *
 * Real OS drag can't be synthesized in Playwright, so these exercise OUR drop
 * handling by emitting the E2E-gated `e2e-trigger-file-drop` event, which the
 * app forwards to `ExplorerAPI.triggerFileDrop` → the SAME
 * `dragDrop.handleFileDrop` the live `onDragDropEvent` 'drop' branch runs (the
 * shared destination guard, source-volume resolution, and transfer dialog).
 *
 * Coverage here is the local-only half (no MTP):
 *  - a local→local drop opens the copy dialog with correct counters;
 *  - toggling that dialog to Move SURVIVES the counters (field-bug-1 regression
 *    pin: a local→local move must keep the deep scan, not zero the tallies).
 *
 * The read-only-device refusal and the MTP↔local cross-volume drops live in
 * `mtp-drag-drop-entry.spec.ts` (they need the virtual MTP device, MTP shard).
 *
 * Fixture layout (at $CMDR_E2E_START_PATH):
 *   left/  (file-a.txt, file-b.txt, sub-dir/, bulk/) ; right/  (empty)
 */

import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  dismissOverlay,
  ensureAppReady,
  expectDialogCounters,
  getFixtureRoot,
  readDialogCounters,
  triggerFileDrop,
  TRANSFER_DIALOG,
} from './helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.describe('Programmatic drop entry (local)', () => {
  test('dropping a local file opens the copy dialog with correct counters', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Drop left/file-a.txt (1 KB) onto the right pane (local root).
    await triggerFileDrop(tauriPage, [path.join(fixtureRoot, 'left', 'file-a.txt')], 'right')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    // Drop defaults to Copy.
    const title = await tauriPage.textContent(`${TRANSFER_DIALOG} h2`)
    expect(title).toContain('Copy')

    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })

    await dismissOverlay(tauriPage)
  })

  test('toggling the dropped copy dialog to Move keeps the counters (field bug 1)', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await triggerFileDrop(tauriPage, [path.join(fixtureRoot, 'left', 'file-a.txt')], 'right')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    // Settle on the copy counters first.
    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })

    // Toggle to Move. A local→local move is NOT the same-volume rename fast path,
    // so the deep scan must keep running and the tallies must NOT zero out.
    await tauriPage.evaluate(`(function(){
        var btns = document.querySelectorAll('${TRANSFER_DIALOG} .toggle-option');
        for (var i = 0; i < btns.length; i++) {
            if ((btns[i].textContent || '').trim() === 'Move') { btns[i].click(); return; }
        }
    })()`)

    // The dialog title flips to Move…
    await expect.poll(async () => tauriPage.textContent(`${TRANSFER_DIALOG} h2`), { timeout: 3000 }).toContain('Move')

    // …and the counters survive (state stays `done`/`counting`, never `skipped`,
    // and the file/byte totals are unchanged).
    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })
    const snapshot = await readDialogCounters(tauriPage)
    expect(snapshot?.scanState, 'a local→local move must NOT be the skipped fast path').not.toBe('skipped')

    await dismissOverlay(tauriPage)
  })
})
