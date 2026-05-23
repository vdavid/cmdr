/**
 * E2E tests for the conditional "Overwrite all smaller" / "Overwrite all older"
 * conflict policies, both as upfront radio choices and as per-file dialog
 * buttons.
 *
 * Data-safety invariant pinned here:
 *
 * - OverwriteSmaller: replaces a dest file ONLY when STRICTLY smaller than the
 *   source. Equal-size, larger, or unknown-size dests are kept verbatim.
 * - OverwriteOlder: replaces a dest file ONLY when STRICTLY older than the
 *   source. Equal-mtime, newer, or unknown-mtime dests are kept verbatim.
 *
 * Both policies must apply per-file (not bulk-skip on first encounter) so a
 * multi-file copy partitions correctly between overwrite and skip.
 */

import fs from 'fs'
import path from 'path'

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { dispatchMenuCommand, ensureAppReady, getFixtureRoot, TRANSFER_DIALOG } from './helpers.js'
import {
  clearFixtureDirs,
  clickConflictButton,
  clickTransferStart,
  readFile,
  selectAll,
  selectConflictPolicy,
  waitForConflictPolicy,
  waitForDialogsToClose,
  writeFile,
} from './conflict-helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

/**
 * Sets a file's modified time to `unixSeconds`. The reducer compares whole-second
 * granularity on the volume side and `SystemTime` on local FS, but for an FS test
 * on macOS / Linux a second-level stamp round-trips cleanly.
 */
function setMtime(fixtureRoot: string, relPath: string, unixSeconds: number): void {
  const full = path.join(fixtureRoot, relPath)
  fs.utimesSync(full, unixSeconds, unixSeconds)
}

/**
 * Layout: three files in left/ and three same-named files in right/.
 *
 * - `smaller.txt`: source is 100 bytes, dest is 50 bytes → STRICTLY smaller dest
 * - `equal.txt`: both 100 bytes → equal-size dest
 * - `larger.txt`: source is 100 bytes, dest is 500 bytes → strictly larger dest
 */
function createSizeFixture(fixtureRoot: string): void {
  clearFixtureDirs(fixtureRoot)
  const src100 = 'a'.repeat(100)
  writeFile(fixtureRoot, 'left/smaller.txt', src100)
  writeFile(fixtureRoot, 'left/equal.txt', src100)
  writeFile(fixtureRoot, 'left/larger.txt', src100)
  writeFile(fixtureRoot, 'right/smaller.txt', 'd'.repeat(50))
  writeFile(fixtureRoot, 'right/equal.txt', 'd'.repeat(100))
  writeFile(fixtureRoot, 'right/larger.txt', 'd'.repeat(500))
}

/**
 * Layout: three files with controlled mtimes.
 *
 * - `older.txt`: source mtime 2024, dest mtime 2020 → STRICTLY older dest
 * - `equal.txt`: both mtime 2022 → equal-mtime dest
 * - `newer.txt`: source mtime 2020, dest mtime 2024 → strictly newer dest
 *
 * Content sizes differ so we can verify which side ended up on disk by reading
 * a single byte (`src-` prefix vs `dst-` prefix). Mtimes are seconds-since-epoch.
 */
function createMtimeFixture(fixtureRoot: string): void {
  clearFixtureDirs(fixtureRoot)
  writeFile(fixtureRoot, 'left/older.txt', 'src-older')
  writeFile(fixtureRoot, 'left/equal.txt', 'src-equal')
  writeFile(fixtureRoot, 'left/newer.txt', 'src-newer')
  writeFile(fixtureRoot, 'right/older.txt', 'dst-older')
  writeFile(fixtureRoot, 'right/equal.txt', 'dst-equal')
  writeFile(fixtureRoot, 'right/newer.txt', 'dst-newer')

  // 2024 source vs 2020 dest for `older.txt` → dest strictly older
  setMtime(fixtureRoot, 'left/older.txt', 1_700_000_000)
  setMtime(fixtureRoot, 'right/older.txt', 1_600_000_000)
  // Equal mtimes for `equal.txt` → no overwrite under OverwriteOlder
  setMtime(fixtureRoot, 'left/equal.txt', 1_650_000_000)
  setMtime(fixtureRoot, 'right/equal.txt', 1_650_000_000)
  // 2020 source vs 2024 dest for `newer.txt` → dest is fresher, must NOT overwrite
  setMtime(fixtureRoot, 'left/newer.txt', 1_600_000_000)
  setMtime(fixtureRoot, 'right/newer.txt', 1_700_000_000)
}

test.describe('Conditional conflict policies (upfront radios)', () => {
  test('Overwrite all smaller: only strictly-smaller dest is replaced', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createSizeFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['equal.txt', 'larger.txt', 'smaller.txt'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite_smaller')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Smaller dest → overwritten with source content (100 'a's).
    expect(readFile(fixtureRoot, 'right/smaller.txt')).toBe('a'.repeat(100))
    // Equal-size dest → kept verbatim (100 'd's).
    expect(readFile(fixtureRoot, 'right/equal.txt')).toBe('d'.repeat(100))
    // Larger dest → kept verbatim (500 'd's). This is the critical safety case:
    // overwriting a larger file under "Overwrite all smaller" would mean data loss.
    expect(readFile(fixtureRoot, 'right/larger.txt')).toBe('d'.repeat(500))
  })

  test('Overwrite all older: only strictly-older dest is replaced', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createMtimeFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['equal.txt', 'newer.txt', 'older.txt'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite_older')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Older dest → overwritten with source content.
    expect(readFile(fixtureRoot, 'right/older.txt')).toBe('src-older')
    // Equal-mtime dest → kept verbatim.
    expect(readFile(fixtureRoot, 'right/equal.txt')).toBe('dst-equal')
    // Newer dest → MUST be kept (the user's fresher file). Replacing it under
    // "Overwrite all older" would be data loss.
    expect(readFile(fixtureRoot, 'right/newer.txt')).toBe('dst-newer')
  })
})

test.describe('Conditional conflict policies (per-file dialog buttons)', () => {
  test('"Overwrite all older" button in per-file dialog applies to all remaining conflicts', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createMtimeFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['equal.txt', 'newer.txt', 'older.txt'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'stop') // "Ask for each"
    await clickTransferStart(tauriPage)

    // Per-file conflict dialog appears for the first conflict. The button is
    // labelled "Overwrite all older" and sends `applyToAll: true`. After
    // clicking it, the rest of the operation runs without further prompts.
    await expect.poll(async () => tauriPage.isVisible('.conflict-section'), { timeout: 5000 }).toBeTruthy()
    await clickConflictButton(tauriPage, '.conflict-buttons-row button', 'Overwrite all older')

    await waitForDialogsToClose(tauriPage)

    // Same expectations as the upfront-radio test: the conditional rule applies
    // per-file across the rest of the batch.
    expect(readFile(fixtureRoot, 'right/older.txt')).toBe('src-older')
    expect(readFile(fixtureRoot, 'right/equal.txt')).toBe('dst-equal')
    expect(readFile(fixtureRoot, 'right/newer.txt')).toBe('dst-newer')
  })
})
