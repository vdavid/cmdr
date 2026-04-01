/**
 * E2E tests for file watching (inotify on Linux, FSEvents on macOS).
 *
 * Verifies that external filesystem changes are detected by the app's
 * file watcher and the pane listing refreshes automatically.
 *
 * Covers: single file/dir CRUD, batch creation, the 500-event
 * full-reread threshold, watched-directory deletion, cross-pane sync,
 * synthetic-diff dedup after in-app copy, and hidden-file filtering.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  ensureAppReady,
  getFixtureRoot,
  fileExistsInFocusedPane,
  fileExistsInPane,
  countEntriesInPane,
  countEntriesWithPrefix,
  getSizeText,
  moveCursorToFile,
  executeViaCommandPalette,
  pollUntil,
  sleep,
  TRANSFER_DIALOG,
} from './helpers.js'

test.describe('File watching', () => {
  /** Paths created outside the fixture's left/ and right/ that need manual cleanup. */
  const extraPaths: string[] = []

  test.beforeEach(() => {
    recreateFixtures(getFixtureRoot())
  })

  test.afterEach(() => {
    for (const p of extraPaths) {
      try {
        fs.rmSync(p, { recursive: true, force: true })
      } catch {
        // Best-effort cleanup
      }
    }
    extraPaths.length = 0
  })

  // ── Single-entry detection ──────────────────────────────────────────────

  test('detects an externally created directory', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const dirName = `watch-test-dir-${Date.now()}`
    const dirPath = path.join(fixtureRoot, 'left', dirName)

    expect(await fileExistsInFocusedPane(tauriPage, dirName)).toBe(false)

    fs.mkdirSync(dirPath)

    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, dirName), 8000)
  })

  test('detects an externally created file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const fileName = `watch-file-${Date.now()}.txt`
    const filePath = path.join(fixtureRoot, 'left', fileName)

    expect(await fileExistsInFocusedPane(tauriPage, fileName)).toBe(false)

    fs.writeFileSync(filePath, 'hello world — watch test')

    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, fileName), 8000)
  })

  test('detects an externally deleted file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // file-a.txt is part of the fixture, should be visible
    expect(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')).toBe(true)

    fs.unlinkSync(path.join(fixtureRoot, 'left', 'file-a.txt'))

    await pollUntil(
      tauriPage,
      async () => !(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')),
      8000,
    )
  })

  test('detects an externally renamed file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    expect(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')).toBe(true)

    fs.renameSync(
      path.join(fixtureRoot, 'left', 'file-a.txt'),
      path.join(fixtureRoot, 'left', 'file-a-renamed.txt'),
    )

    // Both old name gone AND new name present
    await pollUntil(tauriPage, async () => {
      const oldGone = !(await fileExistsInFocusedPane(tauriPage, 'file-a.txt'))
      const newPresent = await fileExistsInFocusedPane(tauriPage, 'file-a-renamed.txt')
      return oldGone && newPresent
    }, 8000)
  })

  test('updates displayed size when a file is modified externally', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch to Full view so the size column is visible
    await executeViaCommandPalette(tauriPage, 'Full view')
    await pollUntil(tauriPage, async () => tauriPage.isVisible('.full-list-container'), 5000)

    const fixtureRoot = getFixtureRoot()

    // Capture the initial size text for file-a.txt (1 024 bytes)
    const initialSize = await getSizeText(tauriPage, 'file-a.txt')
    expect(initialSize).not.toBe('')

    // Append 50 KB to make the size visibly different
    fs.appendFileSync(path.join(fixtureRoot, 'left', 'file-a.txt'), 'X'.repeat(50_000))

    // Wait for the watcher to update the entry with the new size
    await pollUntil(tauriPage, async () => {
      const newSize = await getSizeText(tauriPage, 'file-a.txt')
      return newSize !== '' && newSize !== initialSize
    }, 8000)
  })

  // ── Batch operations ────────────────────────────────────────────────────

  test('detects batch creation of 25 files', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const prefix = `batch-${Date.now()}`

    // Create 25 files in a tight loop
    for (let i = 0; i < 25; i++) {
      fs.writeFileSync(
        path.join(fixtureRoot, 'left', `${prefix}-${String(i).padStart(3, '0')}.txt`),
        `content ${i}`,
      )
    }

    // All 25 should appear in the listing
    await pollUntil(
      tauriPage,
      async () => (await countEntriesWithPrefix(tauriPage, prefix)) === 25,
      10000,
    )
  })

  test('handles 600+ files crossing the full-reread threshold', async ({ tauriPage }, testInfo) => {
    testInfo.setTimeout(60000)
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const leftDir = path.join(fixtureRoot, 'left')

    // Create 600 files directly in left/ where the watcher is already active.
    // This exceeds the 500-event threshold, triggering the full-reread path
    // instead of the incremental path.
    for (let i = 0; i < 600; i++) {
      fs.writeFileSync(
        path.join(leftDir, `mass-${String(i).padStart(4, '0')}.txt`),
        `content ${i}`,
      )
    }

    // Verify files appear in the focused pane
    const found = await pollUntil(
      tauriPage,
      async () => fileExistsInFocusedPane(tauriPage, 'mass-0000.txt'),
      30000,
    )
    expect(found).toBe(true)
  })

  // ── Edge cases ──────────────────────────────────────────────────────────

  test('handles deletion of the watched directory', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Create a temporary directory and navigate the right pane into it
    const tempDir = path.join(fixtureRoot, 'temp-watch-target')
    fs.mkdirSync(tempDir)
    fs.writeFileSync(path.join(tempDir, 'temp-file.txt'), 'temporary content')
    extraPaths.push(tempDir)

    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'right', path: ${JSON.stringify(tempDir)} }
        })`)
    await sleep(500)
    await pollUntil(
      tauriPage,
      async () => fileExistsInPane(tauriPage, 'temp-file.txt', 1),
      8000,
    )

    // Delete the directory externally while the pane is watching it
    fs.rmSync(tempDir, { recursive: true, force: true })
    extraPaths.length = 0 // Already removed

    // Give the app time to react
    await sleep(3000)

    // The app should still be functional — left pane unaffected
    expect(await fileExistsInPane(tauriPage, 'file-a.txt', 0)).toBe(true)

    // Keyboard still works (no crash)
    await tauriPage.keyboard.press('Tab')
    await sleep(300)
    await tauriPage.keyboard.press('Tab')
    await sleep(300)
    expect(await fileExistsInPane(tauriPage, 'file-a.txt', 0)).toBe(true)
  })

  test('updates both panes when both watch the same directory', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const leftDir = path.join(fixtureRoot, 'left')

    // Navigate right pane to the same directory as left pane (left/)
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'right', path: ${JSON.stringify(leftDir)} }
        })`)
    await sleep(500)
    await pollUntil(
      tauriPage,
      async () => fileExistsInPane(tauriPage, 'file-a.txt', 1),
      8000,
    )

    // Create a new file externally
    const fileName = `dual-pane-${Date.now()}.txt`
    fs.writeFileSync(path.join(leftDir, fileName), 'dual pane test')

    // Both panes should show the new file
    await pollUntil(tauriPage, async () => {
      const inLeft = await fileExistsInPane(tauriPage, fileName, 0)
      const inRight = await fileExistsInPane(tauriPage, fileName, 1)
      return inLeft && inRight
    }, 8000)
  })

  test('in-app copy shows file in target pane without duplicates', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Copy file-a.txt from left pane to right pane
    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await pollUntil(
      tauriPage,
      async () => !(await tauriPage.isVisible('.modal-overlay')),
      10000,
    )

    // File should appear in right pane
    await pollUntil(
      tauriPage,
      async () => fileExistsInPane(tauriPage, 'file-a.txt', 1),
      5000,
    )

    // Wait for the watcher to fire after the synthetic diff, then verify
    // there is exactly one entry (no duplicate from watcher re-adding it).
    await sleep(2000)
    const count = await tauriPage.evaluate<number>(`(function() {
            var pane = document.querySelectorAll('.file-pane')[1];
            if (!pane) return 0;
            var entries = pane.querySelectorAll('.file-entry');
            var c = 0;
            for (var i = 0; i < entries.length; i++) {
                var name = (entries[i].querySelector('.col-name') || entries[i].querySelector('.name') || {}).textContent || '';
                if (name === 'file-a.txt') c++;
            }
            return c;
        })()`)
    expect(count).toBe(1)
  })

  test('respects hidden file visibility for externally created dotfiles', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Ensure hidden files are visible first (so we can verify the watcher adds them)
    const hiddenFilesShown = await fileExistsInFocusedPane(tauriPage, '.hidden-file')
    if (!hiddenFilesShown) {
      await executeViaCommandPalette(tauriPage, 'Toggle hidden')
      await pollUntil(
        tauriPage,
        async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'),
        5000,
      )
    }

    // Create a new hidden file externally
    const hiddenName = '.hidden-watch-test'
    fs.writeFileSync(path.join(fixtureRoot, 'left', hiddenName), 'hidden content')

    // It should appear (hidden files are visible)
    await pollUntil(
      tauriPage,
      async () => fileExistsInFocusedPane(tauriPage, hiddenName),
      8000,
    )

    // Toggle hidden files OFF — the dotfile should disappear
    await executeViaCommandPalette(tauriPage, 'Toggle hidden')
    await pollUntil(
      tauriPage,
      async () => !(await fileExistsInFocusedPane(tauriPage, hiddenName)),
      5000,
    )

    // Restore original state
    if (hiddenFilesShown) {
      await executeViaCommandPalette(tauriPage, 'Toggle hidden')
      await pollUntil(
        tauriPage,
        async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'),
        5000,
      )
    }
  })
})
