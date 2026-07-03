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
  dismissOverlay,
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  expectDialogCounters,
  getEntryName,
  getFixtureRoot,
  fileExistsInFocusedPane,
  fileExistsInPane,
  moveCursorToFile,
  executeViaCommandPalette,
  MKDIR_DIALOG,
  NEW_FILE_DIALOG,
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

    // The dialog's counter line must report the single 1 KB file (no dirs).
    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })

    // Click the Copy button
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)

    // Wait for dialog to close (confirms copy succeeded)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 3000 }).toBeTruthy()

    // Switch to right pane to verify the file appeared in DOM
    await tauriPage.keyboard.press('Tab')

    await expect
      .poll(
        async () => {
          const cls = await tauriPage.evaluate<string>(
            `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
          )
          return cls.includes('is-focused')
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'file-a.txt'), { timeout: 5000 }).toBeTruthy()

    // Verify on disk
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt'))).toBe(true)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)

    // F5 of a single cursored file fires the selection-split toast. Asserting on
    // it pins the user-facing confirmation (we ship the wording on purpose).
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')
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

    // A local→local move keeps the deep scan running (NOT the same-volume rename
    // fast path), so the counter line still reports the single 1 KB file.
    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })

    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 3000 }).toBeTruthy()

    // Verify file-b.txt is gone from left pane DOM
    await expect.poll(async () => !(await fileExistsInPane(tauriPage, 'file-b.txt', 0)), { timeout: 5000 }).toBeTruthy()

    // Switch to right pane and verify file-b.txt appeared
    await tauriPage.keyboard.press('Tab')

    await expect
      .poll(
        async () => {
          const cls = await tauriPage.evaluate<string>(
            `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
          )
          return cls.includes('is-focused')
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'file-b.txt'), { timeout: 5000 }).toBeTruthy()

    // Verify on disk
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-b.txt'))).toBe(false)
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-b.txt'))).toBe(true)

    // F6 of a single cursored file fires the selection-split toast.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.')
  })
})

test.describe('Rename round-trip', () => {
  test('renames file-a.txt to renamed-file.txt via F2', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F2')

    // 3 s: the rename input appears in <100 ms on the happy path. Previous
    // 10 s budget exceeded the suite's 8 s per-test ceiling.
    await tauriPage.waitForSelector('.rename-input', 3000)

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
    // Wait for Svelte to flush the reactive update that mirrors the cleared input.
    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`document.querySelector('.rename-input')?.value === ''`), {
        timeout: 2000,
      })
      .toBeTruthy()
    await tauriPage.type('.rename-input', 'renamed-file.txt')
    // Wait until the typed value is fully reflected in the input before Enter.
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`document.querySelector('.rename-input')?.value === 'renamed-file.txt'`),
        { timeout: 3000 },
      )
      .toBeTruthy()
    await tauriPage.press('.rename-input', 'Enter')

    // Wait for rename input to disappear
    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 5000 }).toBeTruthy()

    // Verify new name appears
    await expect
      .poll(async () => fileExistsInFocusedPane(tauriPage, 'renamed-file.txt'), { timeout: 5000 })
      .toBeTruthy()

    // Verify old name is gone (poll because file watcher updates are async)
    await expect
      .poll(async () => !(await fileExistsInFocusedPane(tauriPage, 'file-a.txt')), { timeout: 5000 })
      .toBeTruthy()

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
    // Wait for the OK button to enable in response to the typed name
    await expect.poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: 2000 }).toBeTruthy()

    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    // Verify folder appears in listing
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, folderName), { timeout: 5000 }).toBeTruthy()

    // Verify on disk
    const folderPath = path.join(fixtureRoot, 'left', folderName)
    expect(fs.existsSync(folderPath)).toBe(true)
    expect(fs.statSync(folderPath).isDirectory()).toBe(true)
  })

  test('cursor lands on the newly created folder', async ({ tauriPage }) => {
    // The synthetic directory-diff for the new entry is emitted with a 50 ms
    // trailing-window coalesce (see listing/diff_emitter.rs). After mkdir, the
    // optimistic setCursorIndex landed the cursor correctly, but the deferred
    // diff then ran through the structural cursor-adjustment path and shifted
    // it one row down. This assertion is the regression guard for that race.
    await ensureAppReady(tauriPage)

    // Pick a name that sorts in the middle of the existing dirs so an off-by-one
    // cursor shift produces a different filename. Fixture dirs sorted Asc:
    // bulk, sub-dir. "mid-..." sorts between them.
    const folderName = `mid-cursor-folder-${String(Date.now())}`

    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, folderName)
    await expect.poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: 2000 }).toBeTruthy()
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Dialog closes and the listing renders the new folder. fileExistsInFocusedPane
    // polls the DOM, so by the time it returns true the diff has been applied.
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, folderName), { timeout: 5000 }).toBeTruthy()

    // Cursor must be on the new folder and stay there. Five checks at 80 ms
    // intervals cover both the immediate-post-diff window and any later
    // re-render that might shift the cursor.
    for (let i = 0; i < 5; i++) {
      const cursorName = await tauriPage.evaluate<string>(
        `document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')?.getAttribute('data-filename') || ''`,
      )
      expect(cursorName, `cursor moved off ${folderName} on iteration ${String(i)}`).toBe(folderName)
      if (i < 4) await new Promise((r) => setTimeout(r, 80))
    }
  })
})

test.describe('New file round-trip', () => {
  test('creates a new file via the file.newFile command and lands the cursor on it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const fileName = `new-test-file-${String(Date.now())}.txt`

    // Dispatch the command rather than synthesizing Shift+F4: this test asserts
    // on the resulting round-trip (create → listing → disk → cursor), not the
    // keyboard pathway (docs/testing.md § "Synthesized F-key dispatches"). mkfile
    // is now a managed instant op (routed through the operation manager), so this
    // exercises that path end to end.
    await dispatchMenuCommand(tauriPage, 'file.newFile')
    await tauriPage.waitForSelector(NEW_FILE_DIALOG, 5000)

    await tauriPage.waitForSelector(`${NEW_FILE_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${NEW_FILE_DIALOG} .name-input`, fileName)
    // The OK button enables once the typed name validates (async conflict check).
    await expect
      .poll(async () => tauriPage.isEnabled(`${NEW_FILE_DIALOG} .btn-primary`), { timeout: 2000 })
      .toBeTruthy()

    await tauriPage.click(`${NEW_FILE_DIALOG} .btn-primary`)

    // Dialog closes.
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    // The new file appears in the focused pane's listing.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, fileName), { timeout: 5000 }).toBeTruthy()

    // It exists on disk as an empty regular file.
    const filePath = path.join(fixtureRoot, 'left', fileName)
    expect(fs.existsSync(filePath)).toBe(true)
    expect(fs.statSync(filePath).isFile()).toBe(true)

    // The cursor lands on the new file and stays there. Same 50 ms trailing-window
    // synthetic-diff coalesce race the mkdir cursor test guards against; five
    // checks at 80 ms cover the immediate window and any later re-render shift.
    for (let i = 0; i < 5; i++) {
      const cursorName = await tauriPage.evaluate<string>(
        `document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')?.getAttribute('data-filename') || ''`,
      )
      expect(cursorName, `cursor moved off ${fileName} on iteration ${String(i)}`).toBe(fileName)
      if (i < 4) await new Promise((r) => setTimeout(r, 80))
    }
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

    await expect.poll(async () => tauriPage.isVisible('.full-list-container'), { timeout: 5000 }).toBeTruthy()

    // Full mode should have a header row
    expect(await tauriPage.isVisible('.full-list-container .header-row')).toBe(true)

    // Switch to Brief view
    await executeViaCommandPalette(tauriPage, 'Brief view')

    await expect.poll(async () => tauriPage.isVisible('.brief-list-container'), { timeout: 5000 }).toBeTruthy()
  })
})

test.describe('Hidden files toggle', () => {
  test('toggles hidden file visibility', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Use tauriPage.keyboard to dispatch trusted key events through the webview.
    // Synthetic dispatchEvent() fires with isTrusted:false and may not reach the
    // handler depending on event target (document vs window).
    //
    // Each call site polls for the resulting hidden-file visibility change, so
    // we don't need a fixed-duration settle here; the outer polls cover the
    // IPC dispatch + virtual-scroll refresh.
    const toggleHidden = async () => {
      await tauriPage.keyboard.down(CTRL_OR_META)
      await tauriPage.keyboard.down('Shift')
      await tauriPage.keyboard.press('.')
      await tauriPage.keyboard.up('Shift')
      await tauriPage.keyboard.up(CTRL_OR_META)
    }

    // Ensure hidden files are visible first. On macOS the default state
    // may not have propagated to the DOM yet (async IPC + virtual scroll),
    // so poll and toggle if needed rather than trusting the initial render.
    const hiddenVisibleAtStart = await fileExistsInFocusedPane(tauriPage, '.hidden-file')
    if (!hiddenVisibleAtStart) {
      await toggleHidden()
      await expect.poll(async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'), { timeout: 3000 }).toBeTruthy()
    }

    // Now hidden files are visible, so toggle them OFF
    await toggleHidden()
    await expect
      .poll(async () => !(await fileExistsInFocusedPane(tauriPage, '.hidden-file')), { timeout: 3000 })
      .toBeTruthy()
    expect(await fileExistsInFocusedPane(tauriPage, '.hidden-file')).toBe(false)

    // Toggle back ON so the hidden file should reappear
    await toggleHidden()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, '.hidden-file'), { timeout: 3000 }).toBeTruthy()
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
    await expect
      .poll(
        async () => {
          const count = await tauriPage.count('.palette-overlay .result-item')
          return count > 0
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

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
    await dismissOverlay(tauriPage)
  })
})

test.describe('Empty directory', () => {
  test('shows empty right pane gracefully without crash', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch to right pane (which starts empty)
    await tauriPage.keyboard.press('Tab')

    await expect
      .poll(
        async () => {
          const cls = await tauriPage.evaluate<string>(
            `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
          )
          return cls.includes('is-focused')
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

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

    await expect
      .poll(
        async () => {
          const cls = await tauriPage.evaluate<string>(
            `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
          )
          return cls.includes('is-focused')
        },
        { timeout: 3000 },
      )
      .toBeTruthy()

    const leftPaneClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
    )
    expect(leftPaneClass).toContain('is-focused')
  })
})
