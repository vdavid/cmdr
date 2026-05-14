/**
 * E2E tests for conflict resolution during move (F6) operations.
 *
 * Covers: Move with Overwrite All, Move with Skip All, and Move rollback.
 * Uses Layout B (multi-item merge with partial directory overlaps).
 */

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { dispatchMenuCommand, ensureAppReady, getFixtureRoot, pollUntil, TRANSFER_DIALOG } from './helpers.js'
import {
  createConflictFixturesB,
  readFile,
  fileExists,
  selectAll,
  waitForConflictPolicy,
  selectConflictPolicy,
  clickTransferStart,
  clickConflictButton,
  waitForDialogsToClose,
} from './conflict-helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.describe('Move multi-item merge (Layout B)', () => {
  test('Move multi-item with Overwrite All merges and removes source', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesB(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['alpha'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Dest files correct (same as copy overwrite)
    expect(readFile(fixtureRoot, 'right/alpha/info.txt')).toBe('alpha-info')
    expect(readFile(fixtureRoot, 'right/bravo/payload.txt')).toBe('bravo-payload')
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/golf.txt')).toBe('source-golf')
    expect(readFile(fixtureRoot, 'right/charlie/data.txt')).toBe('charlie-data')
    expect(readFile(fixtureRoot, 'right/delta.txt')).toBe('delta-content')

    // Dest-only files survived the merge
    expect(readFile(fixtureRoot, 'right/bravo/echo.txt')).toBe('bravo-echo')
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/hotel.txt')).toBe('bravo-hotel')

    // Source items removed after move
    expect(fileExists(fixtureRoot, 'left/alpha')).toBe(false)
    expect(fileExists(fixtureRoot, 'left/charlie')).toBe(false)
    expect(fileExists(fixtureRoot, 'left/delta.txt')).toBe(false)
    expect(fileExists(fixtureRoot, 'left/bravo/payload.txt')).toBe(false)
    expect(fileExists(fixtureRoot, 'left/bravo/foxtrot/golf.txt')).toBe(false)
  })

  test('Move multi-item with Skip preserves source of skipped files', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesB(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['alpha'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Dest files correct (same as copy skip)
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/golf.txt')).toBe('dest-golf')
    expect(readFile(fixtureRoot, 'right/alpha/info.txt')).toBe('alpha-info')
    expect(readFile(fixtureRoot, 'right/bravo/payload.txt')).toBe('bravo-payload')
    expect(readFile(fixtureRoot, 'right/charlie/data.txt')).toBe('charlie-data')
    expect(readFile(fixtureRoot, 'right/delta.txt')).toBe('delta-content')

    // Dest-only files survived the merge
    expect(readFile(fixtureRoot, 'right/bravo/echo.txt')).toBe('bravo-echo')
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/hotel.txt')).toBe('bravo-hotel')

    // Skipped file's source still exists (it was not moved)
    expect(fileExists(fixtureRoot, 'left/bravo/foxtrot/golf.txt')).toBe(true)

    // Non-conflicting items were moved (source gone)
    expect(fileExists(fixtureRoot, 'left/alpha')).toBe(false)
    expect(fileExists(fixtureRoot, 'left/charlie')).toBe(false)
    expect(fileExists(fixtureRoot, 'left/delta.txt')).toBe(false)
  })
})

test.describe('Move rollback', () => {
  test('Move rollback button is available and cancels operation', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesB(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['alpha'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Use "Ask for each" to pause on conflict and test rollback from there
    await clickTransferStart(tauriPage)
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 3000)

    // Wait for conflict dialog (bravo/foxtrot/golf.txt conflicts)
    const conflictAppeared = await pollUntil(tauriPage, async () => tauriPage.isVisible('.conflict-section'), 3000)
    expect(conflictAppeared).toBe(true)

    // Verify the Rollback button is shown (not just "Cancel"). Poll briefly —
    // the button container becomes visible a frame or two before its children
    // render under Svelte; a one-shot read can miss it.
    const hasRollback = await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
          var btns = document.querySelectorAll('.conflict-cancel button');
          for (var i=0; i<btns.length; i++) {
            if ((btns[i].textContent || '').trim() === 'Rollback') return true;
          }
          return false;
        })()`),
      2000,
    )
    expect(hasRollback).toBe(true)

    // Click Rollback to cancel the move mid-conflict. Retry on empty NodeList
    // for the same reason as the hasRollback poll above.
    await clickConflictButton(tauriPage, '.conflict-cancel button', 'Rollback')

    await waitForDialogsToClose(tauriPage)

    // Non-conflicting items that were already moved before the conflict
    // may or may not have been rolled back (depends on what was processed
    // before the conflict paused the operation). The key assertion is that
    // the operation was stopped and the dialog closed cleanly.
    // Source items that weren't yet processed should still exist.
    // The conflicting file (golf.txt) should still be in source since
    // we cancelled before resolving it.
    expect(fileExists(fixtureRoot, 'left/bravo/foxtrot/golf.txt')).toBe(true)
  })
})
