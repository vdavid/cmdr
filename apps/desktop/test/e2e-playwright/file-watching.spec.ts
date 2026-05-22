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
  dispatchMenuCommand,
  ensureAppReady,
  flushFileWatcher,
  getFixtureRoot,
  fileExistsInFocusedPane,
  fileExistsInPane,
  countEntriesWithPrefix,
  getSizeText,
  moveCursorToFile,
  executeViaCommandPalette,
  pollUntil,
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

    const dirName = `watch-test-dir-${String(Date.now())}`
    const dirPath = path.join(fixtureRoot, 'left', dirName)

    expect(await fileExistsInFocusedPane(tauriPage, dirName)).toBe(false)

    fs.mkdirSync(dirPath)
    await flushFileWatcher(tauriPage)

    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, dirName), 2000)
  })

  test('detects an externally created file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const fileName = `watch-file-${String(Date.now())}.txt`
    const filePath = path.join(fixtureRoot, 'left', fileName)

    expect(await fileExistsInFocusedPane(tauriPage, fileName)).toBe(false)

    fs.writeFileSync(filePath, 'hello world (watch test)')
    await flushFileWatcher(tauriPage)

    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, fileName), 2000)
  })

  test('detects an externally deleted file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // file-a.txt is part of the fixture, should be visible
    expect(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')).toBe(true)

    fs.unlinkSync(path.join(fixtureRoot, 'left', 'file-a.txt'))
    await flushFileWatcher(tauriPage)

    await pollUntil(tauriPage, async () => !(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')), 2000)
  })

  test('detects an externally renamed file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    expect(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')).toBe(true)

    fs.renameSync(path.join(fixtureRoot, 'left', 'file-a.txt'), path.join(fixtureRoot, 'left', 'file-a-renamed.txt'))
    await flushFileWatcher(tauriPage)

    // Both old name gone AND new name present
    await pollUntil(
      tauriPage,
      async () => {
        const oldGone = !(await fileExistsInFocusedPane(tauriPage, 'file-a.txt'))
        const newPresent = await fileExistsInFocusedPane(tauriPage, 'file-a-renamed.txt')
        return oldGone && newPresent
      },
      2000,
    )
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
    await flushFileWatcher(tauriPage)

    // Wait for the watcher to update the entry with the new size
    await pollUntil(
      tauriPage,
      async () => {
        const newSize = await getSizeText(tauriPage, 'file-a.txt')
        return newSize !== '' && newSize !== initialSize
      },
      2000,
    )
  })

  // ── Batch operations ────────────────────────────────────────────────────

  test('detects batch creation of 25 files', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const prefix = `batch-${String(Date.now())}`

    // Create 25 files in a tight loop
    for (let i = 0; i < 25; i++) {
      fs.writeFileSync(
        path.join(fixtureRoot, 'left', `${prefix}-${String(i).padStart(3, '0')}.txt`),
        `content ${String(i)}`,
      )
    }
    await flushFileWatcher(tauriPage)

    // All 25 should appear in the listing
    await pollUntil(tauriPage, async () => (await countEntriesWithPrefix(tauriPage, prefix)) === 25, 2000)
  })

  // `flush_file_watcher` bypasses the OS watcher entirely (it calls
  // `handle_directory_change` synchronously), so macOS's slower FSEvents path
  // no longer matters for this test. Previously gated on `process.platform !==
  // 'darwin'`; flipped to unconditional now that the flush path is mature.
  test('handles 600+ files crossing the full-reread threshold', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const leftDir = path.join(fixtureRoot, 'left')

    // Create 600 files directly in left/ where the watcher is already active.
    // This exceeds the 500-event threshold, triggering the full-reread path
    // instead of the incremental path.
    for (let i = 0; i < 600; i++) {
      fs.writeFileSync(path.join(leftDir, `mass-${String(i).padStart(4, '0')}.txt`), `content ${String(i)}`)
    }
    await flushFileWatcher(tauriPage)

    // Verify files appear in the focused pane
    const found = await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, 'mass-0000.txt'), 5000)
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
    await pollUntil(tauriPage, async () => fileExistsInPane(tauriPage, 'temp-file.txt', 1), 2000)

    // Delete the directory externally while the pane is watching it
    fs.rmSync(tempDir, { recursive: true, force: true })
    extraPaths.length = 0 // Already removed
    await flushFileWatcher(tauriPage)

    // Poll until the file-pane stops listing the temp file (proves the listing
    // was refreshed). The subsequent assertions cover the "app still works"
    // contract regardless.
    await pollUntil(
      tauriPage,
      async () => {
        const stillThere = await fileExistsInPane(tauriPage, 'temp-file.txt', 1)
        return !stillThere
      },
      2000,
    )

    // The app should still be functional: left pane unaffected
    expect(await fileExistsInPane(tauriPage, 'file-a.txt', 0)).toBe(true)

    // Keyboard still works (no crash): pressing Tab twice returns focus to the
    // left pane; we verify by re-asserting the left-pane file is still listed.
    await tauriPage.keyboard.press('Tab')
    await tauriPage.keyboard.press('Tab')
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
    await pollUntil(tauriPage, async () => fileExistsInPane(tauriPage, 'file-a.txt', 1), 2000)

    // Create a new file externally
    const fileName = `dual-pane-${String(Date.now())}.txt`
    fs.writeFileSync(path.join(leftDir, fileName), 'dual pane test')
    await flushFileWatcher(tauriPage)

    // Both panes should show the new file
    await pollUntil(
      tauriPage,
      async () => {
        const inLeft = await fileExistsInPane(tauriPage, fileName, 0)
        const inRight = await fileExistsInPane(tauriPage, fileName, 1)
        return inLeft && inRight
      },
      2000,
    )
  })

  test('in-app copy shows file in target pane without duplicates', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Copy file-a.txt from left pane to right pane
    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await dispatchMenuCommand(tauriPage, 'file.copy')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
    await flushFileWatcher(tauriPage)

    // File should appear in right pane, and after the watcher fires there
    // should be exactly one instance (no duplicate from watcher re-adding it).
    // Poll instead of a fixed sleep: the DOM can be transiently empty during
    // a watcher-triggered re-render.
    const noDuplicates = await pollUntil(
      tauriPage,
      async () => {
        const count = await tauriPage.evaluate<number>(`(function() {
              var pane = document.querySelectorAll('.file-pane')[1];
              if (!pane) return 0;
              return pane.querySelectorAll('[data-filename="file-a.txt"]').length;
          })()`)
        return count === 1
      },
      3000,
    )
    expect(noDuplicates).toBe(true)
  })

  test('respects hidden file visibility for externally created dotfiles', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Ensure hidden files are visible first (so we can verify the watcher adds them)
    const hiddenFilesShown = await fileExistsInFocusedPane(tauriPage, '.hidden-file')
    if (!hiddenFilesShown) {
      await executeViaCommandPalette(tauriPage, 'Toggle hidden')
      await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'), 3000)
    }

    // Create a new hidden file externally
    const hiddenName = '.hidden-watch-test'
    fs.writeFileSync(path.join(fixtureRoot, 'left', hiddenName), 'hidden content')
    await flushFileWatcher(tauriPage)

    // It should appear (hidden files are visible)
    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, hiddenName), 2000)

    // Toggle hidden files OFF; the dotfile should disappear
    await executeViaCommandPalette(tauriPage, 'Toggle hidden')
    await pollUntil(tauriPage, async () => !(await fileExistsInFocusedPane(tauriPage, hiddenName)), 3000)

    // Restore original state
    if (hiddenFilesShown) {
      await executeViaCommandPalette(tauriPage, 'Toggle hidden')
      await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'), 3000)
    }
  })
})
