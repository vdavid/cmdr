/**
 * E2E tests for conflict resolution during copy (F5) operations.
 *
 * Covers: Overwrite All, Skip All, per-file decisions, Rename, and Rename All
 * across two fixture layouts (A: nested conflicts, B: multi-item merge).
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  countTree,
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  expectDialogCounters,
  getFixtureRoot,
  moveCursorToFile,
  pollUntil,
  TRANSFER_DIALOG,
} from './helpers.js'

/** Recursive file/dir counts for everything `selectAll` grabs in `left/`
 *  (the top-level children, including the preserved `bulk/` tree). */
function leftSelectionCounts(fixtureRoot: string): { files: number; dirs: number } {
  const leftDir = path.join(fixtureRoot, 'left')
  const children = fs.readdirSync(leftDir).map((name) => path.join(leftDir, name))
  return countTree(children)
}
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
  clickConflictButton,
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
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    // The counter line reports the full recursive selection (Layout A text files
    // + the preserved bulk/ tree). Computed from disk so it tracks the fixtures.
    await expectDialogCounters(tauriPage, leftSelectionCounts(fixtureRoot))
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)
    // Layout A selectAll copies 2 top-level files + 2 folders (docs/, bulk/); the
    // completion toast must be asserted + dismissed or the afterEach leak guard
    // fails on the still-visible transient toast.
    await expectAndDismissToast(tauriPage, 'Copied 2 files and 2 folders.')

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
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await expectDialogCounters(tauriPage, leftSelectionCounts(fixtureRoot))
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)
    // Skip All keeps the conflicting top-level readme.txt at the dest, so the
    // toast reports it as skipped (only-in-source.txt + docs/ + bulk/ copied).
    await expectAndDismissToast(tauriPage, 'Copied 1 file and 2 folders, skipped 1 file')

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
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await expectDialogCounters(tauriPage, leftSelectionCounts(fixtureRoot))
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)
    // Layout B selectAll: 1 top-level file (delta.txt) + 4 folders (alpha, bravo,
    // charlie, bulk).
    await expectAndDismissToast(tauriPage, 'Copied 1 file and 4 folders.')

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
    await dispatchMenuCommand(tauriPage, 'file.copy')

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
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Leave default "Ask for each" (value="stop") and start
    await clickTransferStart(tauriPage)

    // Wait for progress dialog with inline conflict UI
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 3000)

    // Wait for first conflict to appear
    const conflictAppeared = await pollUntil(tauriPage, async () => tauriPage.isVisible('.conflict-section'), 3000)
    expect(conflictAppeared).toBe(true)

    // Capture the first conflict's filename so we can poll for the next one
    // (the `.conflict-section` stays mounted between conflicts; only its
    // `.conflict-filename` content changes).
    const firstConflictName = await tauriPage.evaluate<string>(
      `(document.querySelector('.conflict-section .conflict-filename')?.textContent || '').trim()`,
    )

    // First conflict: click "Overwrite" (single file, not "Overwrite all").
    // `.conflict-section` being visible doesn't guarantee Svelte has rendered
    // the inner buttons yet, so retry via clickConflictButton until the click
    // actually lands.
    await clickConflictButton(tauriPage, '.conflict-buttons-row button', 'Overwrite')

    // Wait for the next conflict (different filename) or for the conflict UI
    // to disappear (no more conflicts).
    const firstNameJson = JSON.stringify(firstConflictName)
    const nextConflict = await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(
          `(function(){
            var el = document.querySelector('.conflict-section .conflict-filename');
            if (!el) return true;
            var name = (el.textContent || '').trim();
            return name !== ${firstNameJson};
          })()`,
        ),
      3000,
    )
    // After the wait, `.conflict-section` might still be visible (next conflict)
    // or gone (no more conflicts). We proceed if there's a new conflict to act on.
    const stillVisible = await tauriPage.isVisible('.conflict-section')
    if (nextConflict && stillVisible) {
      await clickConflictButton(tauriPage, '.conflict-buttons-row button', 'Skip all')
    }

    await waitForDialogsToClose(tauriPage)
    // The copy fires a transient selection-split toast. Layout A's selectAll
    // grabs 2 top-level files (readme.txt, only-in-source.txt) + 2 folders
    // (docs/, bulk/); per-file Skips are not surfaced in the count. Assert +
    // dismiss it so the afterEach leak-detector does not fail on a still-visible
    // toast (these per-file-conflict flows finish slower than the upfront-policy
    // tests, so the toast is still on screen when afterEach probes — it does not
    // reliably auto-dismiss in time on Linux).
    await expectAndDismissToast(tauriPage, 'Copied 2 files and 2 folders.')

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
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Single cursored 1 KB file (file-a.txt), no dirs.
    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })

    // Use "Ask for each" (default=stop) to get the inline conflict dialog
    await clickTransferStart(tauriPage)

    // Wait for progress dialog with conflict
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 3000)
    const conflictAppeared = await pollUntil(tauriPage, async () => tauriPage.isVisible('.conflict-section'), 3000)
    expect(conflictAppeared).toBe(true)

    // Click "Rename": keeps both files, incoming gets " (1)" suffix
    await clickConflictButton(tauriPage, '.conflict-buttons-row button', 'Rename')

    await waitForDialogsToClose(tauriPage)
    // Single cursored file copied (Rename keeps both → counts as one file).
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')

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
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)

    // Use "Ask for each" to get inline dialog, then click "Rename all"
    await clickTransferStart(tauriPage)
    await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 3000)

    const conflictAppeared = await pollUntil(tauriPage, async () => tauriPage.isVisible('.conflict-section'), 3000)
    expect(conflictAppeared).toBe(true)

    await clickConflictButton(tauriPage, '.conflict-buttons-row button', 'Rename all')

    await waitForDialogsToClose(tauriPage)
    // Layout A selectAll: 2 top-level files + 2 folders (docs/, bulk/).
    await expectAndDismissToast(tauriPage, 'Copied 2 files and 2 folders.')

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
