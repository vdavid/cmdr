/**
 * E2E tests for conflict resolution during copy (F5) operations.
 *
 * Covers: Overwrite All, Skip All, per-file decisions, Rename, and Rename All
 * across two fixture layouts (A: nested conflicts, B: multi-item merge).
 */

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, getFixtureRoot, moveCursorToFile, pollUntil, sleep, TRANSFER_DIALOG } from './helpers.js'
import {
  createConflictFixturesA,
  createConflictFixturesB,
  readFile,
  writeFile,
  fileExists,
  selectAll,
  waitForConflictPolicy,
  selectConflictPolicy,
  clickTransferStart,
  waitForDialogsToClose,
} from './conflict-helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.describe('Copy with conflict policies (Layout A)', () => {
  test('Copy with Overwrite All resolves nested conflicts', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesA(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['readme.txt'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Conflicting files overwritten with source content
    expect(readFile(fixtureRoot, 'right/readme.txt')).toBe('source-readme')
    expect(readFile(fixtureRoot, 'right/docs/guide.txt')).toBe('source-guide')
    expect(readFile(fixtureRoot, 'right/docs/nested/config.txt')).toBe('source-config')

    // Non-conflicting source files copied
    expect(readFile(fixtureRoot, 'right/only-in-source.txt')).toBe('only-in-source')
    expect(readFile(fixtureRoot, 'right/docs/only-in-source-deep.txt')).toBe('only-in-source-deep')

    // Dest-only files survived the merge
    expect(readFile(fixtureRoot, 'right/only-in-dest.txt')).toBe('only-in-dest')
    expect(readFile(fixtureRoot, 'right/docs/only-in-dest-deep.txt')).toBe('only-in-dest-deep')
  })

  test('Copy with Skip All preserves destination files', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesA(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['readme.txt'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Conflicting files preserved (dest content kept)
    expect(readFile(fixtureRoot, 'right/readme.txt')).toBe('dest-readme')
    expect(readFile(fixtureRoot, 'right/docs/guide.txt')).toBe('dest-guide')
    expect(readFile(fixtureRoot, 'right/docs/nested/config.txt')).toBe('dest-config')

    // Non-conflicting source files still copied
    expect(readFile(fixtureRoot, 'right/only-in-source.txt')).toBe('only-in-source')
    expect(readFile(fixtureRoot, 'right/docs/only-in-source-deep.txt')).toBe('only-in-source-deep')

    // Dest-only files survived
    expect(readFile(fixtureRoot, 'right/only-in-dest.txt')).toBe('only-in-dest')
  })
})

test.describe('Copy multi-item merge (Layout B)', () => {
  test('Copy multi-item with Overwrite All merges correctly', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesB(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['alpha'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // New dirs and files created
    expect(readFile(fixtureRoot, 'right/alpha/info.txt')).toBe('alpha-info')
    expect(readFile(fixtureRoot, 'right/charlie/data.txt')).toBe('charlie-data')
    expect(readFile(fixtureRoot, 'right/delta.txt')).toBe('delta-content')

    // New file in existing dir
    expect(readFile(fixtureRoot, 'right/bravo/payload.txt')).toBe('bravo-payload')

    // Conflicting file overwritten
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/golf.txt')).toBe('source-golf')

    // Dest-only files survived
    expect(readFile(fixtureRoot, 'right/bravo/echo.txt')).toBe('bravo-echo')
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/hotel.txt')).toBe('bravo-hotel')
  })

  test('Copy multi-item with Skip preserves conflicting files', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesB(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['alpha'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Conflicting file preserved
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/golf.txt')).toBe('dest-golf')

    // Non-conflicting files copied
    expect(readFile(fixtureRoot, 'right/alpha/info.txt')).toBe('alpha-info')
    expect(readFile(fixtureRoot, 'right/bravo/payload.txt')).toBe('bravo-payload')
    expect(readFile(fixtureRoot, 'right/charlie/data.txt')).toBe('charlie-data')
    expect(readFile(fixtureRoot, 'right/delta.txt')).toBe('delta-content')

    // Dest-only files survived
    expect(readFile(fixtureRoot, 'right/bravo/echo.txt')).toBe('bravo-echo')
    expect(readFile(fixtureRoot, 'right/bravo/foxtrot/hotel.txt')).toBe('bravo-hotel')
  })
})

test.describe('Per-file conflict decisions (Layout A)', () => {
  test('Copy with mixed per-file conflict decisions', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesA(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['readme.txt'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Leave default "Ask for each" (value="stop") and start
    await clickTransferStart(tauriPage)

    // Wait for progress dialog with inline conflict UI
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 10000)

    // Wait for first conflict to appear
    const conflictAppeared = await pollUntil(
      tauriPage,
      async () => tauriPage.isVisible('.conflict-section'),
      15000,
    )
    expect(conflictAppeared).toBe(true)

    // First conflict: click "Overwrite" (single file, not "Overwrite all")
    await tauriPage.evaluate(`(function(){
      var btns = document.querySelectorAll('.conflict-buttons-row button');
      for (var i=0; i<btns.length; i++) {
        if (btns[i].textContent.trim() === 'Overwrite') { btns[i].click(); break; }
      }
    })()`)

    // Wait for next conflict or brief re-render
    await sleep(500)

    // Second conflict: click "Skip all" (applies to all remaining)
    const nextConflict = await pollUntil(
      tauriPage,
      async () => tauriPage.isVisible('.conflict-section'),
      10000,
    )
    if (nextConflict) {
      await tauriPage.evaluate(`(function(){
        var btns = document.querySelectorAll('.conflict-buttons-row button');
        for (var i=0; i<btns.length; i++) {
          if (btns[i].textContent.trim() === 'Skip all') { btns[i].click(); break; }
        }
      })()`)
    }

    await waitForDialogsToClose(tauriPage)

    // We overwrote the first conflict and skipped the rest.
    // Since filesystem traversal order is unpredictable, verify that
    // exactly ONE of the three conflicting files was overwritten.
    const readmeOverwritten = readFile(fixtureRoot, 'right/readme.txt') === 'source-readme'
    const guideOverwritten = readFile(fixtureRoot, 'right/docs/guide.txt') === 'source-guide'
    const configOverwritten = readFile(fixtureRoot, 'right/docs/nested/config.txt') === 'source-config'

    const overwrittenCount = [readmeOverwritten, guideOverwritten, configOverwritten].filter(Boolean).length
    expect(overwrittenCount).toBe(1)

    // Non-conflicting source files should still have been copied
    expect(readFile(fixtureRoot, 'right/only-in-source.txt')).toBe('only-in-source')
    expect(readFile(fixtureRoot, 'right/docs/only-in-source-deep.txt')).toBe('only-in-source-deep')

    // Dest-only files survived
    expect(readFile(fixtureRoot, 'right/only-in-dest.txt')).toBe('only-in-dest')
  })
})

test.describe('Rename conflict resolution', () => {
  test('Copy with Ask-for-each and Rename keeps both files', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    recreateFixtures(fixtureRoot)

    // Create a conflicting file in right/
    writeFile(fixtureRoot, 'right/file-a.txt', 'original-dest')
    await ensureAppReady(tauriPage)

    await moveCursorToFile(tauriPage, 'file-a.txt')
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Use "Ask for each" (default=stop) to get the inline conflict dialog
    await clickTransferStart(tauriPage)

    // Wait for progress dialog with conflict
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 10000)
    const conflictAppeared = await pollUntil(
      tauriPage,
      async () => tauriPage.isVisible('.conflict-section'),
      15000,
    )
    expect(conflictAppeared).toBe(true)

    // Click "Rename" — keeps both files, incoming gets " (1)" suffix
    await tauriPage.evaluate(`(function(){
      var btns = document.querySelectorAll('.conflict-buttons-row button');
      for (var i=0; i<btns.length; i++) {
        if (btns[i].textContent.trim() === 'Rename') { btns[i].click(); break; }
      }
    })()`)

    await waitForDialogsToClose(tauriPage)

    // Original dest file preserved
    expect(readFile(fixtureRoot, 'right/file-a.txt')).toBe('original-dest')

    // Renamed copy exists with source content (1024 'A' chars)
    expect(fileExists(fixtureRoot, 'right/file-a (1).txt')).toBe(true)
    expect(readFile(fixtureRoot, 'right/file-a (1).txt').length).toBe(1024)
  })

  test('Copy with Rename All keeps all files with numbered suffixes', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createConflictFixturesA(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['readme.txt'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Use "Ask for each" to get inline dialog, then click "Rename all"
    await clickTransferStart(tauriPage)
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 10000)

    const conflictAppeared = await pollUntil(
      tauriPage,
      async () => tauriPage.isVisible('.conflict-section'),
      15000,
    )
    expect(conflictAppeared).toBe(true)

    await tauriPage.evaluate(`(function(){
      var btns = document.querySelectorAll('.conflict-buttons-row button');
      for (var i=0; i<btns.length; i++) {
        if (btns[i].textContent.trim() === 'Rename all') { btns[i].click(); break; }
      }
    })()`)

    await waitForDialogsToClose(tauriPage)

    // All original dest files preserved
    expect(readFile(fixtureRoot, 'right/readme.txt')).toBe('dest-readme')
    expect(readFile(fixtureRoot, 'right/docs/guide.txt')).toBe('dest-guide')
    expect(readFile(fixtureRoot, 'right/docs/nested/config.txt')).toBe('dest-config')

    // Renamed copies exist with source content
    expect(readFile(fixtureRoot, 'right/readme (1).txt')).toBe('source-readme')
    expect(readFile(fixtureRoot, 'right/docs/guide (1).txt')).toBe('source-guide')
    expect(readFile(fixtureRoot, 'right/docs/nested/config (1).txt')).toBe('source-config')

    // Non-conflicting files copied normally (no rename needed)
    expect(readFile(fixtureRoot, 'right/only-in-source.txt')).toBe('only-in-source')
    expect(readFile(fixtureRoot, 'right/docs/only-in-source-deep.txt')).toBe('only-in-source-deep')
  })
})
