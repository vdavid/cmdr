/**
 * E2E tests for file operations in the Cmdr Tauri application.
 *
 * These tests verify file operation round-trips: the UI action is performed,
 * then both DOM state and on-disk state are verified. Fixtures are recreated
 * before each test via the shared fixture system.
 *
 * Fixture layout (at $CMDR_E2E_START_PATH):
 *   left/                        <- left pane starts here
 *     file-a.txt, file-b.txt, sub-dir/, bulk/, .hidden-file
 *   right/                       <- right pane starts here (empty)
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  ensureAppReady,
  getEntryName,
  getFixtureRoot,
  fileExistsInFocusedPane,
  fileExistsInPane,
  moveCursorToFile,
  executeViaCommandPalette,
  pollUntil,
  sleep,
  MKDIR_DIALOG,
  TRANSFER_DIALOG,
  CTRL_OR_META,
} from './helpers.js'

// Recreate lightweight fixtures (text files + dirs, not bulk .dat files)
// before each test so file operations don't leak state between tests.
test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe('Copy round-trip', () => {
  test('copies file-a.txt from left pane to right pane via F5', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    const titleText = await tauriPage.textContent(`${TRANSFER_DIALOG} h2`)
    expect(titleText).toContain('Copy')

    // Click the Copy button
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)

    // Wait for dialog to close (confirms copy succeeded)
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 10000)

    // Switch to right pane to verify the file appeared in DOM
    await tauriPage.keyboard.press('Tab')

    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.evaluate<string>(
          `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
        )
        return cls.includes('is-focused')
      },
      3000,
    )

    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, 'file-a.txt'), 5000)

    // Verify on disk
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt'))).toBe(true)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
  })
})

test.describe('Move round-trip', () => {
  test('moves file-b.txt from left pane to right pane via F6', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-b.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    const titleText = await tauriPage.textContent(`${TRANSFER_DIALOG} h2`)
    expect(titleText).toContain('Move')

    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 10000)

    // Verify file-b.txt is gone from left pane DOM
    await pollUntil(tauriPage, async () => !(await fileExistsInPane(tauriPage, 'file-b.txt', 0)), 5000)

    // Switch to right pane and verify file-b.txt appeared
    await tauriPage.keyboard.press('Tab')

    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.evaluate<string>(
          `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
        )
        return cls.includes('is-focused')
      },
      3000,
    )

    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, 'file-b.txt'), 5000)

    // Verify on disk
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-b.txt'))).toBe(false)
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-b.txt'))).toBe(true)
  })
})

test.describe('Rename round-trip', () => {
  test('renames file-a.txt to renamed-file.txt via F2', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F2')

    await tauriPage.waitForSelector('.rename-input', 10000)

    // Clear existing value and type the new name. Use the native value setter
    // + input event to clear (Svelte reads e.target.value in handleInput),
    // then type character by character (triggers proper keydown/input/keyup).
    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.rename-input');
            if (!input) return;
            input.focus();
            var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
            if (desc && desc.set) desc.set.call(input, '');
            else input.value = '';
            input.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)
    await sleep(100)
    await tauriPage.type('.rename-input', 'renamed-file.txt')
    await sleep(200)
    await tauriPage.press('.rename-input', 'Enter')

    // Wait for rename input to disappear
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.rename-input')), 5000)

    // Verify new name appears
    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, 'renamed-file.txt'), 5000)

    // Verify old name is gone (poll because file watcher updates are async)
    await pollUntil(tauriPage, async () => !(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')), 5000)

    // Verify on disk
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'renamed-file.txt'))).toBe(true)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(false)
  })
})

test.describe('Create folder round-trip', () => {
  test('creates a new folder via F7 and verifies on disk', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const folderName = `new-test-folder-${String(Date.now())}`

    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)

    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, folderName)
    await sleep(200)

    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)

    // Verify folder appears in listing
    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, folderName), 5000)

    // Verify on disk
    const folderPath = path.join(fixtureRoot, 'left', folderName)
    expect(fs.existsSync(folderPath)).toBe(true)
    expect(fs.statSync(folderPath).isDirectory()).toBe(true)
  })
})

test.describe('View mode toggle', () => {
  test('switches between Brief and Full view modes', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    const hasBriefList = await tauriPage.isVisible('.brief-list-container')
    const hasFullList = await tauriPage.isVisible('.full-list-container')
    expect(hasBriefList || hasFullList).toBe(true)

    // Switch to Full view via command palette
    await executeViaCommandPalette(tauriPage, 'Full view')

    await pollUntil(tauriPage, async () => tauriPage.isVisible('.full-list-container'), 5000)

    // Full mode should have a header row
    expect(await tauriPage.isVisible('.full-list-container .header-row')).toBe(true)

    // Switch to Brief view
    await executeViaCommandPalette(tauriPage, 'Brief view')

    await pollUntil(tauriPage, async () => tauriPage.isVisible('.brief-list-container'), 5000)
  })
})

test.describe('Hidden files toggle', () => {
  test('toggles hidden file visibility', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Use the Tauri command directly — synthetic keyboard events via dispatchEvent
    // don't reliably reach Tauri's shortcut handler on all runs.
    const toggleHidden = async () => {
      await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('toggle_hidden_files')`)
      await sleep(500)
    }

    // Ensure hidden files are visible first. On macOS the default state
    // may not have propagated to the DOM yet (async IPC + virtual scroll),
    // so poll and toggle if needed rather than trusting the initial render.
    const hiddenVisibleAtStart = await fileExistsInFocusedPane(tauriPage, '.hidden-file')
    if (!hiddenVisibleAtStart) {
      await toggleHidden()
      await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'), 5000)
    }

    // Now hidden files are visible — toggle them OFF
    await toggleHidden()
    await pollUntil(tauriPage, async () => !(await fileExistsInFocusedPane(tauriPage, '.hidden-file')), 5000)
    expect(await fileExistsInFocusedPane(tauriPage, '.hidden-file')).toBe(false)

    // Toggle back ON — hidden file should reappear
    await toggleHidden()
    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'), 5000)
    expect(await fileExistsInFocusedPane(tauriPage, '.hidden-file')).toBe(true)
  })
})

test.describe('Command palette', () => {
  test('opens, shows results, and closes with Escape', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Open command palette via keyboard shortcut
    const isMac = process.platform === 'darwin'
    await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
            key: 'p', ctrlKey: ${String(!isMac)}, metaKey: ${String(isMac)}, shiftKey: true, bubbles: true
        }))`)

    await tauriPage.waitForSelector('.palette-overlay', 5000)

    // Verify the search input exists
    expect(await tauriPage.isVisible('.palette-overlay .search-input')).toBe(true)

    // Verify results container
    expect(await tauriPage.isVisible('.palette-overlay .results-container')).toBe(true)

    // Type a query to filter results
    await tauriPage.fill('.palette-overlay .search-input', 'sort')

    // Wait for filtered results
    await pollUntil(
      tauriPage,
      async () => {
        const count = await tauriPage.count('.palette-overlay .result-item')
        return count > 0
      },
      3000,
    )

    const resultCount = await tauriPage.count('.palette-overlay .result-item')
    expect(resultCount).toBeGreaterThan(0)

    // Verify at least one result contains "sort"
    const hasSortResult = await tauriPage.evaluate<boolean>(`(function() {
            var items = document.querySelectorAll('.palette-overlay .result-item');
            for (var i = 0; i < items.length; i++) {
                if (items[i].textContent.toLowerCase().indexOf('sort') >= 0) return true;
            }
            return false;
        })()`)
    expect(hasSortResult).toBe(true)

    // Close palette with Escape
    await tauriPage.keyboard.press('Escape')

    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.palette-overlay')), 3000)
  })
})

test.describe('Empty directory', () => {
  test('shows empty right pane gracefully without crash', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch to right pane (which starts empty)
    await tauriPage.keyboard.press('Tab')

    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.evaluate<string>(
          `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
        )
        return cls.includes('is-focused')
      },
      3000,
    )

    // Verify right pane is focused
    const rightPaneClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
    )
    expect(rightPaneClass).toContain('is-focused')

    // The pane should render without errors
    const entryCount = await tauriPage.evaluate<number>(
      `document.querySelectorAll('.file-pane')[1]?.querySelectorAll('.file-entry').length || 0`,
    )

    if (entryCount > 0) {
      const firstEntryName = await getEntryName(tauriPage, '.file-pane:nth-child(2) .file-entry:first-child')
      expect(firstEntryName === '..' || firstEntryName === '').toBe(true)
    }

    // Verify no error message
    const hasError = await tauriPage.evaluate<boolean>(
      `!!document.querySelectorAll('.file-pane')[1]?.querySelector('.error-message')`,
    )
    expect(hasError).toBe(false)

    // Verify the pane still renders
    expect(await tauriPage.isVisible('.file-pane.is-focused')).toBe(true)

    // Can still switch back to left pane
    await tauriPage.keyboard.press('Tab')

    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.evaluate<string>(
          `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
        )
        return cls.includes('is-focused')
      },
      3000,
    )

    const leftPaneClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
    )
    expect(leftPaneClass).toContain('is-focused')
  })
})
