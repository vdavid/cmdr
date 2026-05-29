/**
 * E2E matrix for the conflict-dialog state machine.
 *
 * Exercises the per-file conflict dialog ("Ask for each" mode) across clash
 * types and resolution choices, plus the two-bucket apply-to-all latch
 * (`ApplyToAll { normal, file_to_folder }`) that decides which "* all" choices
 * carry over between normal clashes and the destructive file→folder clash.
 *
 * Backend contract under test:
 * `src-tauri/src/file_system/write_operations/helpers.rs`
 *   - `apply_to_all_effective` / `apply_to_all_record` (the bucket rules)
 *   - Skip/Rename carry over normal → file→folder; Overwrite* does NOT.
 *   - A file→folder "* all" picked as the VERY FIRST clash spreads to normal.
 * Frontend dialog: `src/lib/file-operations/transfer/TransferProgressDialog.svelte`
 *   - file→folder variant: red `.conflict-warning`, buttons renamed to
 *     `Overwrite folder with file` / `Overwrite folders with files`.
 *
 * ── Matrix map (test name → matrix cell) ─────────────────────────────────────
 *
 * Axis 1 — single clash × choice × clash type (foundation):
 *   "file→file Overwrite lands source bytes"          file→file  × Overwrite
 *   "file→file Skip keeps dest bytes"                  file→file  × Skip
 *   "folder→folder Overwrite merges into dest"         folder→fldr × Overwrite (inner)
 *   "folder→file Skip keeps the dest file"             folder→file × Skip
 *   "folder→file Rename keeps both"                    folder→file × Rename
 *   "folder→file Overwrite swaps file for folder"      folder→file × Overwrite
 *   "file→folder Skip keeps the dest folder"           file→folder × Skip
 *   "file→folder Rename keeps both"                    file→folder × Rename
 *   "file→folder Overwrite swaps folder for file"      file→folder × Overwrite (renamed btn)
 *   "file→folder Skip all latches file_to_folder"      file→folder × Skip all
 *   "file→folder Overwrite all swaps + latches"        file→folder × Overwrite folders with files
 *
 * Axis 2 — ordered-pair bucket spread (the heart):
 *   normal-first → file→folder, one test per "* all":
 *     "Skip all carries normal → file→folder"          Skip all      → carries (no 2nd prompt)
 *     "Overwrite all does NOT carry to file→folder"    Overwrite all → prompts again
 *     "Overwrite all smaller does NOT carry"           Ovr-smaller   → prompts again
 *     "Overwrite all older does NOT carry"             Ovr-older     → prompts again
 *   mixed independent buckets:
 *     "normal and file→folder buckets latch independently"  f→f Ovr-all, f→fldr Ovr-all, silent follow-ons
 *   (file→folder-first → normal spread is NOT reachable E2E; see the long note
 *    above that describe block. Covered by BE unit tests instead.)
 *
 * Axis 3 — cross-type Overwrite atomicity smoke (no .cmdr-temp survivors):
 *   "folder→file Overwrite leaves no temp artifacts"
 *   "file→folder Overwrite leaves no temp artifacts"
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  getFixtureRoot,
  TRANSFER_DIALOG,
} from './helpers.js'
import {
  createFileOverFileFixture,
  createFolderOverFolderFixture,
  createFolderOverFileFixture,
  createFileOverFolderFixture,
  createOrderedPairFixture,
  clearFixtureDirs,
  writeFile,
  readFile,
  selectItemsByName,
  waitForConflict,
  waitForNextConflictOrDone,
  resolveConflict,
  expectNoTempArtifacts,
  waitForDialogsToClose,
  type ConflictSnapshot,
} from './conflict-helpers.js'

const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'

type PageLike = Parameters<typeof ensureAppReady>[0]

/**
 * Selects exactly `items` in the left pane, fires Copy, leaves the conflict
 * policy at the default ("Ask for each" = stop), and starts the operation so
 * the per-file conflict dialog drives. Returns once the first conflict is on
 * screen. Scopes the op to the named items so the completion toast count and
 * the dest tree stay predictable (no stray `bulk/` or standard fixtures).
 */
async function startCopyAskForEach(tauriPage: PageLike, items: string[]): Promise<ConflictSnapshot> {
  await selectItemsByName(tauriPage, items)
  await dispatchMenuCommand(tauriPage, 'file.copy')
  await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
  // Default policy is "stop" (Ask for each); start straight away.
  await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
  await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
  await tauriPage.waitForSelector(PROGRESS_DIALOG, 5000)
  return waitForConflict(tauriPage)
}

/** Waits for the transfer dialog to close, then asserts + dismisses the
 *  "Copy complete" toast it fires on success (the leak-detector fails the
 *  test otherwise). */
async function finishCopy(tauriPage: PageLike): Promise<void> {
  await waitForDialogsToClose(tauriPage)
  await expectAndDismissToast(tauriPage, 'Copy complete')
}

/** Asserts a directory holds exactly the given child names (no extras, like temp asides). */
function expectDirChildren(dirAbs: string, expected: string[]): void {
  const actual = fs.readdirSync(dirAbs).sort()
  expect(actual).toEqual([...expected].sort())
}

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// ── Axis 1: single clash × choice × clash type ───────────────────────────────

test.describe('Single clash: baseline file/folder smoke', () => {
  test('file→file Overwrite lands source bytes', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFileOverFileFixture(fixtureRoot, 'doc.txt')
    await ensureAppReady(tauriPage, { leftPane: ['doc.txt'] })

    const conflict = await startCopyAskForEach(tauriPage, ['doc.txt'])
    expect(conflict.isFileOverFolder).toBe(false)
    expect(conflict.filename).toBe('doc.txt')
    await resolveConflict(tauriPage, 'Overwrite')
    await finishCopy(tauriPage)

    expect(readFile(fixtureRoot, 'right/doc.txt')).toBe('source-doc.txt')
  })

  test('file→file Skip keeps dest bytes', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFileOverFileFixture(fixtureRoot, 'doc.txt')
    await ensureAppReady(tauriPage, { leftPane: ['doc.txt'] })

    await startCopyAskForEach(tauriPage, ['doc.txt'])
    await resolveConflict(tauriPage, 'Skip')
    await finishCopy(tauriPage)

    expect(readFile(fixtureRoot, 'right/doc.txt')).toBe('dest-doc.txt')
  })

  test('folder→folder Overwrite merges into dest', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFolderOverFolderFixture(fixtureRoot, 'box')
    await ensureAppReady(tauriPage, { leftPane: ['box'] })

    // Top-level folder→folder is a merge, not a replace, so the only prompt
    // is the inner shared.txt file→file clash. Overwrite it.
    const conflict = await startCopyAskForEach(tauriPage, ['box'])
    expect(conflict.isFileOverFolder).toBe(false)
    expect(conflict.filename).toBe('shared.txt')
    await resolveConflict(tauriPage, 'Overwrite')
    await finishCopy(tauriPage)

    // Merge: source-only, dest-only, and the overwritten shared child all present.
    expect(readFile(fixtureRoot, 'right/box/shared.txt')).toBe('source-shared-box')
    expect(readFile(fixtureRoot, 'right/box/only-source.txt')).toBe('only-source-box')
    expect(readFile(fixtureRoot, 'right/box/only-dest.txt')).toBe('only-dest-box')
  })
})

test.describe('Single clash: folder→file (existing file, incoming folder)', () => {
  test('folder→file Skip keeps the dest file', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFolderOverFileFixture(fixtureRoot, 'thing')
    await ensureAppReady(tauriPage, { leftPane: ['thing'] })

    const conflict = await startCopyAskForEach(tauriPage, ['thing'])
    // folder→file is NOT the red file→folder variant.
    expect(conflict.isFileOverFolder).toBe(false)
    await resolveConflict(tauriPage, 'Skip')
    await finishCopy(tauriPage)

    const destPath = path.join(fixtureRoot, 'right', 'thing')
    expect(fs.lstatSync(destPath).isFile()).toBe(true)
    expect(readFile(fixtureRoot, 'right/thing')).toBe('dest-thing')
  })

  test('folder→file Rename keeps both', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFolderOverFileFixture(fixtureRoot, 'thing')
    await ensureAppReady(tauriPage, { leftPane: ['thing'] })

    await startCopyAskForEach(tauriPage, ['thing'])
    await resolveConflict(tauriPage, 'Rename')
    await finishCopy(tauriPage)

    // Original dest file untouched; the incoming folder lands renamed.
    expect(readFile(fixtureRoot, 'right/thing')).toBe('dest-thing')
    expect(fs.lstatSync(path.join(fixtureRoot, 'right', 'thing')).isFile()).toBe(true)
    const renamedDir = path.join(fixtureRoot, 'right', 'thing (1)')
    expect(fs.lstatSync(renamedDir).isDirectory()).toBe(true)
    expect(readFile(fixtureRoot, 'right/thing (1)/sentinel.txt')).toBe('source-sentinel-thing')
  })

  test('folder→file Overwrite swaps file for folder', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFolderOverFileFixture(fixtureRoot, 'thing')
    await ensureAppReady(tauriPage, { leftPane: ['thing'] })

    await startCopyAskForEach(tauriPage, ['thing'])
    await resolveConflict(tauriPage, 'Overwrite')
    await finishCopy(tauriPage)

    const destPath = path.join(fixtureRoot, 'right', 'thing')
    expect(fs.lstatSync(destPath).isDirectory()).toBe(true)
    expect(readFile(fixtureRoot, 'right/thing/sentinel.txt')).toBe('source-sentinel-thing')
    expectNoTempArtifacts(path.join(fixtureRoot, 'right'))
  })
})

test.describe('Single clash: file→folder (existing folder, incoming file — destructive)', () => {
  test('file→folder Skip keeps the dest folder', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFileOverFolderFixture(fixtureRoot, 'item')
    await ensureAppReady(tauriPage, { leftPane: ['item'] })

    const conflict = await startCopyAskForEach(tauriPage, ['item'])
    // This IS the red warning variant.
    expect(conflict.isFileOverFolder).toBe(true)
    await resolveConflict(tauriPage, 'Skip')
    await finishCopy(tauriPage)

    const destPath = path.join(fixtureRoot, 'right', 'item')
    expect(fs.lstatSync(destPath).isDirectory()).toBe(true)
    expect(readFile(fixtureRoot, 'right/item/inside.txt')).toBe('dest-inside-item')
  })

  test('file→folder Rename keeps both', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFileOverFolderFixture(fixtureRoot, 'item')
    await ensureAppReady(tauriPage, { leftPane: ['item'] })

    const conflict = await startCopyAskForEach(tauriPage, ['item'])
    expect(conflict.isFileOverFolder).toBe(true)
    await resolveConflict(tauriPage, 'Rename')
    await finishCopy(tauriPage)

    // Dest folder survives; incoming file lands renamed.
    expect(fs.lstatSync(path.join(fixtureRoot, 'right', 'item')).isDirectory()).toBe(true)
    expect(readFile(fixtureRoot, 'right/item/inside.txt')).toBe('dest-inside-item')
    expect(readFile(fixtureRoot, 'right/item (1)')).toBe('source-item')
  })

  test('file→folder Overwrite swaps folder for file', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFileOverFolderFixture(fixtureRoot, 'item')
    await ensureAppReady(tauriPage, { leftPane: ['item'] })

    const conflict = await startCopyAskForEach(tauriPage, ['item'])
    expect(conflict.isFileOverFolder).toBe(true)
    // Renamed button copy for the destructive variant.
    await resolveConflict(tauriPage, 'Overwrite folder with file')
    await finishCopy(tauriPage)

    const destPath = path.join(fixtureRoot, 'right', 'item')
    expect(fs.lstatSync(destPath).isFile()).toBe(true)
    expect(readFile(fixtureRoot, 'right/item')).toBe('source-item')
    expectNoTempArtifacts(path.join(fixtureRoot, 'right'))
  })

  test('file→folder Skip all latches file_to_folder', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    // Two file→folder clashes; Skip all on the first must silence the second.
    clearFixtureDirs(fixtureRoot)
    writeFile(fixtureRoot, 'left/1-item', 'source-1-item')
    writeFile(fixtureRoot, 'right/1-item/inside.txt', 'dest-inside-1')
    writeFile(fixtureRoot, 'left/2-item', 'source-2-item')
    writeFile(fixtureRoot, 'right/2-item/inside.txt', 'dest-inside-2')
    await ensureAppReady(tauriPage, { leftPane: ['1-item'] })

    const first = await startCopyAskForEach(tauriPage, ['1-item', '2-item'])
    expect(first.isFileOverFolder).toBe(true)
    await resolveConflict(tauriPage, 'Skip all')

    // No second prompt: both dest folders survive untouched.
    const next = await waitForNextConflictOrDone(tauriPage, first)
    expect(next).toBeNull()
    await finishCopy(tauriPage)

    expect(readFile(fixtureRoot, 'right/1-item/inside.txt')).toBe('dest-inside-1')
    expect(readFile(fixtureRoot, 'right/2-item/inside.txt')).toBe('dest-inside-2')
  })

  test('file→folder Overwrite all swaps + latches', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    clearFixtureDirs(fixtureRoot)
    writeFile(fixtureRoot, 'left/1-item', 'source-1-item')
    writeFile(fixtureRoot, 'right/1-item/inside.txt', 'dest-inside-1')
    writeFile(fixtureRoot, 'left/2-item', 'source-2-item')
    writeFile(fixtureRoot, 'right/2-item/inside.txt', 'dest-inside-2')
    await ensureAppReady(tauriPage, { leftPane: ['1-item'] })

    const first = await startCopyAskForEach(tauriPage, ['1-item', '2-item'])
    expect(first.isFileOverFolder).toBe(true)
    await resolveConflict(tauriPage, 'Overwrite folders with files')

    const next = await waitForNextConflictOrDone(tauriPage, first)
    expect(next).toBeNull()
    await finishCopy(tauriPage)

    // Both dest folders replaced by the incoming files; no temp asides survive.
    expect(fs.lstatSync(path.join(fixtureRoot, 'right', '1-item')).isFile()).toBe(true)
    expect(fs.lstatSync(path.join(fixtureRoot, 'right', '2-item')).isFile()).toBe(true)
    expect(readFile(fixtureRoot, 'right/1-item')).toBe('source-1-item')
    expect(readFile(fixtureRoot, 'right/2-item')).toBe('source-2-item')
    expectNoTempArtifacts(path.join(fixtureRoot, 'right'))
  })
})

// ── Axis 2: ordered-pair bucket spread ───────────────────────────────────────
//
// Items are processed in name-ascending order, so `1-normal.txt` (file→file)
// prompts before `2-folder` (file→folder). The first prompt must be the normal
// clash; we assert that explicitly so a sort-order change can't silently make
// the test pass for the wrong reason.

// Observed processing order: when a file→file clash and a file→folder clash
// are in the same operation, the file→file clash always prompts FIRST and the
// file→folder clash second, regardless of name order. (The file→folder clash
// surfaces later in the per-file copy pass.) So the "* all" choice on the
// file→file clash is what may or may not carry over to the file→folder clash.
test.describe('Bucket spread: normal-first → file→folder', () => {
  for (const variant of [
    { choice: 'Skip all', carries: true },
    { choice: 'Overwrite all', carries: false },
    { choice: 'Overwrite all smaller', carries: false },
    { choice: 'Overwrite all older', carries: false },
  ] as const) {
    // `Rename all` carries over too (and now lands correctly for the file→folder
    // clash), but `Skip all` already covers the carry-over contract for this
    // axis, so we don't duplicate it here. The single-clash section pins
    // type-mismatch Rename's landing behavior directly.
    const verb = variant.carries ? 'carries' : 'does NOT carry'
    test(`${variant.choice} ${verb} normal → file→folder`, async ({ tauriPage }) => {
      const fixtureRoot = getFixtureRoot()
      createOrderedPairFixture(fixtureRoot, { normalName: '1-normal.txt', pairName: '2-folder' })
      await ensureAppReady(tauriPage, { leftPane: ['1-normal.txt'] })

      const first = await startCopyAskForEach(tauriPage, ['1-normal.txt', '2-folder'])
      // First clash MUST be the normal (file→file) one.
      expect(first.isFileOverFolder).toBe(false)
      expect(first.filename).toBe('1-normal.txt')
      await resolveConflict(tauriPage, variant.choice)

      const next = await waitForNextConflictOrDone(tauriPage, first)
      if (variant.carries) {
        // Skip all carries over to file→folder: no second prompt.
        expect(next).toBeNull()
      } else {
        // Overwrite* does NOT carry: the destructive file→folder clash prompts.
        expect(next).not.toBeNull()
        expect(next?.isFileOverFolder).toBe(true)
        expect(next?.filename).toBe('2-folder')
        // Resolve it so the op can finish and the dialog closes cleanly.
        await resolveConflict(tauriPage, 'Skip')
      }
      await finishCopy(tauriPage)

      // The dest folder stays intact in every case here (Skip carries; Overwrite*
      // prompted and we Skipped it). The contract under test is whether a SECOND
      // prompt happened, asserted via `next` above.
      const folderPath = path.join(fixtureRoot, 'right', '2-folder')
      expect(fs.lstatSync(folderPath).isDirectory()).toBe(true)
      expect(readFile(fixtureRoot, 'right/2-folder/inside.txt')).toBe('dest-inside-2-folder')
    })
  }
})

// The "file→folder clash is the FIRST clash of the op, so its '* all' spreads to
// the normal bucket" scenario (BE: `apply_to_all_record` with `was_first_clash`)
// is NOT reachable E2E through local copy: a file→file clash always prompts
// before a file→folder clash in the same op (see the note above), and a
// coexisting normal clash is exactly what you'd need to observe the spread. With
// no normal clash there's nothing to observe; with a normal clash it prompts
// first and seeds `has_seen_clash`, so the file→folder choice can't be "first."
// The spread branch is covered by the BE unit tests
// `helpers::tests::file_to_folder_first_overwrite_all_spreads_to_normal` and
// `single_choice_does_not_set_apply_to_all_but_still_seeds_first_clash_flag`.

test.describe('Bucket spread: mixed independent buckets', () => {
  test('normal and file→folder buckets latch independently', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    // Two normal clashes and two file→folder clashes. file→file clashes always
    // prompt before file→folder clashes (observed order), so the first prompt is
    // a normal clash, which seeds has_seen_clash — the later file→folder "* all"
    // therefore can't spread to the normal bucket.
    clearFixtureDirs(fixtureRoot)
    // 1-norm.txt: file→file (first clash → normal bucket)
    writeFile(fixtureRoot, 'left/1-norm.txt', 'source-1-norm')
    writeFile(fixtureRoot, 'right/1-norm.txt', 'dest-1-norm')
    // 2-fold: file→folder (second prompt → file_to_folder bucket, must NOT spread)
    writeFile(fixtureRoot, 'left/2-fold', 'source-2-fold')
    writeFile(fixtureRoot, 'right/2-fold/inside.txt', 'dest-inside-2')
    // 3-norm.txt: file→file (should follow normal bucket = Overwrite, silent)
    writeFile(fixtureRoot, 'left/3-norm.txt', 'source-3-norm')
    writeFile(fixtureRoot, 'right/3-norm.txt', 'dest-3-norm')
    // 4-fold: file→folder (should follow file_to_folder bucket = Skip, silent)
    writeFile(fixtureRoot, 'left/4-fold', 'source-4-fold')
    writeFile(fixtureRoot, 'right/4-fold/inside.txt', 'dest-inside-4')
    await ensureAppReady(tauriPage, { leftPane: ['1-norm.txt'] })

    const first = await startCopyAskForEach(tauriPage, ['1-norm.txt', '2-fold', '3-norm.txt', '4-fold'])
    expect(first.isFileOverFolder).toBe(false)
    expect(first.filename).toBe('1-norm.txt')
    await resolveConflict(tauriPage, 'Overwrite all')

    // Normal Overwrite all does NOT carry to file→folder, so 2-fold prompts.
    const second = await waitForNextConflictOrDone(tauriPage, first)
    expect(second).not.toBeNull()
    expect(second?.isFileOverFolder).toBe(true)
    expect(second?.filename).toBe('2-fold')
    await resolveConflict(tauriPage, 'Overwrite folders with files')

    // After that: 3-norm.txt follows normal-bucket Overwrite (silent), and
    // 4-fold follows file_to_folder-bucket Overwrite (silent). No more prompts.
    if (second) {
      const third = await waitForNextConflictOrDone(tauriPage, second)
      expect(third).toBeNull()
    }
    await finishCopy(tauriPage)

    // Normal bucket = Overwrite: both file→file clashes took source bytes.
    expect(readFile(fixtureRoot, 'right/1-norm.txt')).toBe('source-1-norm')
    expect(readFile(fixtureRoot, 'right/3-norm.txt')).toBe('source-3-norm')
    // file_to_folder bucket = Overwrite folders with files: both swapped.
    expect(fs.lstatSync(path.join(fixtureRoot, 'right', '2-fold')).isFile()).toBe(true)
    expect(fs.lstatSync(path.join(fixtureRoot, 'right', '4-fold')).isFile()).toBe(true)
    expect(readFile(fixtureRoot, 'right/2-fold')).toBe('source-2-fold')
    expect(readFile(fixtureRoot, 'right/4-fold')).toBe('source-4-fold')
    expectNoTempArtifacts(path.join(fixtureRoot, 'right'))
  })
})

// ── Axis 3: cross-type Overwrite atomicity smoke ─────────────────────────────

test.describe('Cross-type Overwrite atomicity', () => {
  test('folder→file Overwrite leaves no temp artifacts', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFolderOverFileFixture(fixtureRoot, 'swap')
    await ensureAppReady(tauriPage, { leftPane: ['swap'] })

    await startCopyAskForEach(tauriPage, ['swap'])
    await resolveConflict(tauriPage, 'Overwrite')
    await finishCopy(tauriPage)

    const dest = path.join(fixtureRoot, 'right', 'swap')
    expect(fs.lstatSync(dest).isDirectory()).toBe(true)
    expect(readFile(fixtureRoot, 'right/swap/sentinel.txt')).toBe('source-sentinel-swap')
    expectDirChildren(path.join(fixtureRoot, 'right'), ['swap'])
    expectNoTempArtifacts(path.join(fixtureRoot, 'right'))
  })

  test('file→folder Overwrite leaves no temp artifacts', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createFileOverFolderFixture(fixtureRoot, 'swap')
    await ensureAppReady(tauriPage, { leftPane: ['swap'] })

    const conflict = await startCopyAskForEach(tauriPage, ['swap'])
    expect(conflict.isFileOverFolder).toBe(true)
    await resolveConflict(tauriPage, 'Overwrite folder with file')
    await finishCopy(tauriPage)

    const dest = path.join(fixtureRoot, 'right', 'swap')
    expect(fs.lstatSync(dest).isFile()).toBe(true)
    expect(readFile(fixtureRoot, 'right/swap')).toBe('source-swap')
    expectDirChildren(path.join(fixtureRoot, 'right'), ['swap'])
    expectNoTempArtifacts(path.join(fixtureRoot, 'right'))
  })
})
