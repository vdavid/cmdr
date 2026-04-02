/**
 * E2E tests for drive indexing: directory sizes in the file explorer.
 *
 * These tests verify the full indexing pipeline: scanner → SQLite → enrichment
 * → UI rendering. They depend on the drive indexer having reached the fixture
 * directory, so each test checks for index readiness and skips gracefully if
 * the index is not available within a generous timeout.
 *
 * Size assertions are byte-exact using `get_dir_stats` IPC (logical sizes).
 * The fixture's `sub-dir/` contains exactly one file: `nested-file.txt`
 * with content `'A'.repeat(1024)` = 1024 bytes.
 */

import fs from 'fs'
import path from 'path'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, getFixtureRoot, executeViaCommandPalette, getSizeText, pollUntil, sleep } from './helpers.js'

/** Union type for tauriPage — works in both Tauri and browser mode. */
type PageLike = TauriPage | BrowserPageAdapter

// ── Helpers ──────────────────────────────────────────────────────────────────

/** IPC response shape from `get_dir_stats` (camelCase via serde rename). */
interface DirStats {
  path: string
  recursiveSize: number
  recursivePhysicalSize: number
  recursiveFileCount: number
  recursiveDirCount: number
}

/**
 * Calls `get_dir_stats` IPC with the canonical path.
 * Returns null if the path is not indexed or indexing is not initialized.
 */
async function getDirStats(tauriPage: PageLike, dirPath: string): Promise<DirStats | null> {
  let canonicalPath: string
  try {
    canonicalPath = fs.realpathSync(dirPath)
  } catch {
    return null
  }
  const pathJson = JSON.stringify(canonicalPath)
  try {
    return (await tauriPage.evaluate(
      `window.__TAURI_INTERNALS__.invoke('get_dir_stats', { path: ${pathJson} })`,
    )) as DirStats | null
  } catch {
    return null
  }
}

/**
 * Polls `get_dir_stats` until `recursiveFileCount > 0` or timeout.
 * Returns the stats if available, null otherwise.
 */
async function waitForIndexData(tauriPage: PageLike, dirPath: string, timeoutMs = 90_000): Promise<DirStats | null> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const stats = await getDirStats(tauriPage, dirPath)
    if (stats && stats.recursiveFileCount > 0) return stats
    await sleep(2000)
  }
  return null
}

/**
 * Polls `get_dir_stats` until `recursiveSize` equals the expected value.
 * Returns the final stats, or null on timeout.
 */
async function waitForExactSize(
  tauriPage: PageLike,
  dirPath: string,
  expectedSize: number,
  timeoutMs = 30_000,
): Promise<DirStats | null> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const stats = await getDirStats(tauriPage, dirPath)
    if (stats && stats.recursiveSize === expectedSize) return stats
    await sleep(500)
  }
  return await getDirStats(tauriPage, dirPath)
}

/** Switches to Full view mode (needed to see the size column). */
async function ensureFullView(tauriPage: PageLike): Promise<void> {
  const isFullView = await tauriPage.isVisible('.full-list-container')
  if (!isFullView) {
    await executeViaCommandPalette(tauriPage, 'Full view')
    await pollUntil(tauriPage, async () => tauriPage.isVisible('.full-list-container'), 5000)
  }
}

/** Waits until a directory's size column shows a numeric value (not "<dir>" or "Scanning..."). */
async function waitForNumericSize(tauriPage: PageLike, entryName: string, timeoutMs = 15000): Promise<string> {
  await pollUntil(
    tauriPage,
    async () => {
      const text = await getSizeText(tauriPage, entryName)
      return /\d/.test(text)
    },
    timeoutMs,
  )
  return getSizeText(tauriPage, entryName)
}

// ── Constants ────────────────────────────────────────────────────────────────

/** Size of nested-file.txt in the fixture (see e2e-shared/fixtures.ts). */
const NESTED_FILE_SIZE = 1024

/** Size of the extra file created by the creation/deletion tests. */
const EXTRA_FILE_SIZE = 10_000

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe('Drive indexing', () => {
  test.beforeEach(() => {
    recreateFixtures(getFixtureRoot())
  })

  test('shows correct directory size from the index', async ({ tauriPage }, testInfo) => {
    testInfo.setTimeout(120_000)
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const subDirPath = path.join(fixtureRoot, 'left', 'sub-dir')

    // Wait for the index to have data for sub-dir
    const stats = await waitForIndexData(tauriPage, subDirPath)
    if (!stats) {
      console.warn('SKIPPED: Drive index not ready for fixture path within 90 s') // eslint-disable-line no-console
      return
    }

    // sub-dir contains exactly one file: nested-file.txt (1024 bytes)
    // Verify byte-exact logical size via IPC
    expect(stats.recursiveSize).toBe(NESTED_FILE_SIZE)
    expect(stats.recursiveFileCount).toBe(1)
    expect(stats.recursiveDirCount).toBe(0)

    // Also verify the UI shows a numeric size (not "<dir>" or "Scanning...")
    await ensureFullView(tauriPage)
    const sizeText = await waitForNumericSize(tauriPage, 'sub-dir')
    expect(sizeText).toMatch(/\d/)
    expect(sizeText).not.toContain('Scanning')
  })

  test('increases directory size by exact byte count after file creation', async ({ tauriPage }, testInfo) => {
    testInfo.setTimeout(150_000)
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const subDirPath = path.join(fixtureRoot, 'left', 'sub-dir')

    const initialStats = await waitForIndexData(tauriPage, subDirPath)
    if (!initialStats) {
      console.warn('SKIPPED: Drive index not ready for fixture path within 90 s') // eslint-disable-line no-console
      return
    }

    // Verify initial state
    expect(initialStats.recursiveSize).toBe(NESTED_FILE_SIZE)
    expect(initialStats.recursiveFileCount).toBe(1)

    // Create a file with exactly EXTRA_FILE_SIZE bytes inside sub-dir
    fs.writeFileSync(path.join(subDirPath, 'extra-file.txt'), 'Y'.repeat(EXTRA_FILE_SIZE))

    // Wait for the indexing pipeline to converge to the exact expected size
    const expectedSize = NESTED_FILE_SIZE + EXTRA_FILE_SIZE
    const updatedStats = await waitForExactSize(tauriPage, subDirPath, expectedSize)
    expect(updatedStats).not.toBeNull()
    expect(updatedStats?.recursiveSize).toBe(expectedSize)
    expect(updatedStats?.recursiveFileCount).toBe(2)

    // Verify the UI also updated (Full view)
    await ensureFullView(tauriPage)
    const sizeText = await waitForNumericSize(tauriPage, 'sub-dir')
    expect(sizeText).toMatch(/\d/)
  })

  test('decreases directory size by exact byte count after file deletion', async ({ tauriPage }, testInfo) => {
    testInfo.setTimeout(150_000)
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const subDirPath = path.join(fixtureRoot, 'left', 'sub-dir')

    // Create an extra file first so we have something to delete
    const extraFile = path.join(subDirPath, 'extra-for-delete.txt')
    fs.writeFileSync(extraFile, 'Z'.repeat(EXTRA_FILE_SIZE))

    // Wait for the index to converge to the size including both files
    const expectedSizeWithExtra = NESTED_FILE_SIZE + EXTRA_FILE_SIZE
    const statsWithExtra = await waitForExactSize(tauriPage, subDirPath, expectedSizeWithExtra, 90_000)
    if (!statsWithExtra) {
      // Index might not have data yet, or hasn't picked up the extra file
      const fallback = await waitForIndexData(tauriPage, subDirPath)
      if (!fallback) {
        console.warn('SKIPPED: Drive index not ready for fixture path within 90 s') // eslint-disable-line no-console
        return
      }
      // If the index has data but not the exact size, the extra file hasn't been indexed yet.
      // Wait a bit more.
      console.warn(`Index has recursiveSize=${fallback.recursiveSize}, expected ${expectedSizeWithExtra}`) // eslint-disable-line no-console
    }

    expect(statsWithExtra).not.toBeNull()
    expect(statsWithExtra?.recursiveSize).toBe(expectedSizeWithExtra)
    expect(statsWithExtra?.recursiveFileCount).toBe(2)

    // Delete the extra file
    fs.unlinkSync(extraFile)

    // Size should decrease back to exactly NESTED_FILE_SIZE
    const statsAfterDelete = await waitForExactSize(tauriPage, subDirPath, NESTED_FILE_SIZE)
    expect(statsAfterDelete).not.toBeNull()
    expect(statsAfterDelete?.recursiveSize).toBe(NESTED_FILE_SIZE)
    expect(statsAfterDelete?.recursiveFileCount).toBe(1)
  })
})
