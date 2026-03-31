/**
 * E2E tests for the Cmdr Tauri application — basic rendering, keyboard
 * navigation, mouse interactions, navigation, and dialogs.
 *
 * Uses tauri-playwright in Tauri mode: commands are injected directly into
 * the real Tauri webview via Unix socket. No WebDriver, no quirks.
 */

import { test, expect } from './fixtures.js'
import {
  ensureAppReady,
  findFileIndex,
  skipParentEntry,
  pollUntil,
  pressKey,
  sleep,
  MKDIR_DIALOG,
  TRANSFER_DIALOG,
} from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

// ─── Helpers ────────────────────────────────────────────────────────────────

/**
 * Moves the cursor to "sub-dir" using keyboard commands.
 */
async function moveCursorToSubDir(tauriPage: PageLike): Promise<boolean> {
  const info = await findFileIndex(tauriPage, 'sub-dir')
  if ('error' in info || info.targetIndex < 0) return false

  await tauriPage.keyboard.press('Home')
  await sleep(100)
  for (let i = 0; i < info.targetIndex; i++) {
    await tauriPage.keyboard.press('ArrowDown')
    await sleep(50)
  }
  await sleep(100)
  return true
}

// ─── Tests ──────────────────────────────────────────────────────────────────

test.describe('Basic rendering', () => {
  test('launches and shows the main window', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const title = await tauriPage.title()
    expect(title).toContain('Cmdr')
  })

  test('displays the dual pane interface', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const paneCount = await tauriPage.count('.file-pane')
    expect(paneCount).toBe(2)
  })

  test('shows file entries in the panes', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const entryCount = await tauriPage.count('.file-entry')
    expect(entryCount).toBeGreaterThan(0)
  })
})

test.describe('Keyboard navigation', () => {
  test('moves cursor with arrow keys', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    const entryCount = await tauriPage.count('.file-entry')
    if (entryCount < 2) return

    // Find initial cursor position
    const initialCursorIndex = await tauriPage.evaluate<number>(`(function() {
            var entries = document.querySelectorAll('.file-entry');
            for (var i = 0; i < entries.length; i++) {
                if (entries[i].classList.contains('is-under-cursor')) return i;
            }
            return -1;
        })()`)

    expect(initialCursorIndex).toBeGreaterThanOrEqual(0)

    // Press ArrowDown to move cursor
    await tauriPage.keyboard.press('ArrowDown')

    // Wait for cursor position to change
    await pollUntil(
      tauriPage,
      async () => {
        const newIndex = await tauriPage.evaluate<number>(`(function() {
                    var entries = document.querySelectorAll('.file-entry');
                    for (var i = 0; i < entries.length; i++) {
                        if (entries[i].classList.contains('is-under-cursor')) return i;
                    }
                    return -1;
                })()`)
        return newIndex >= 0 && newIndex !== initialCursorIndex
      },
      3000,
    )

    const newCursorIndex = await tauriPage.evaluate<number>(`(function() {
            var entries = document.querySelectorAll('.file-entry');
            for (var i = 0; i < entries.length; i++) {
                if (entries[i].classList.contains('is-under-cursor')) return i;
            }
            return -1;
        })()`)

    expect(newCursorIndex).toBeGreaterThanOrEqual(0)
    expect(newCursorIndex).not.toBe(initialCursorIndex)
  })

  test('switches panes with Tab key', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    const paneCount = await tauriPage.count('.file-pane')
    expect(paneCount).toBe(2)

    // Verify left pane is focused (ensureAppReady clicked on it)
    const leftPaneClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
    )
    expect(leftPaneClass).toContain('is-focused')

    // Press Tab to switch to right pane
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

    // Verify right pane is now focused
    const rightPaneClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
    )
    expect(rightPaneClass).toContain('is-focused')

    const leftPaneClassAfter = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
    )
    expect(leftPaneClassAfter).not.toContain('is-focused')

    // Press Tab again to go back to left pane
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

    const leftPaneClassFinal = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[0]?.getAttribute('class') || ''`,
    )
    expect(leftPaneClassFinal).toContain('is-focused')
  })

  test('toggles selection with Space key', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Skip ".." entry if that's where cursor is
    await skipParentEntry(tauriPage)

    // Verify not selected initially
    let cursorClass = await tauriPage.getAttribute('.file-entry.is-under-cursor', 'class')
    expect(cursorClass).not.toContain('is-selected')

    // Press Space to toggle selection
    await pressKey(tauriPage, 'Space')

    // Wait for selection state to change
    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.getAttribute('.file-entry.is-under-cursor', 'class')
        return cls?.includes('is-selected') ?? false
      },
      3000,
    )

    cursorClass = await tauriPage.getAttribute('.file-entry.is-under-cursor', 'class')
    expect(cursorClass).toContain('is-selected')

    // Press Space again to deselect
    await pressKey(tauriPage, 'Space')

    await pollUntil(
      tauriPage,
      async () => {
        const cls = await tauriPage.getAttribute('.file-entry.is-under-cursor', 'class')
        return !(cls?.includes('is-selected') ?? false)
      },
      3000,
    )

    cursorClass = await tauriPage.getAttribute('.file-entry.is-under-cursor', 'class')
    expect(cursorClass).not.toContain('is-selected')
  })
})

test.describe('Mouse interactions', () => {
  test('moves cursor when clicking a file entry', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    const leftPaneEntryCount = await tauriPage.evaluate<number>(
      `document.querySelectorAll('.file-pane')[0]?.querySelectorAll('.file-entry').length || 0`,
    )
    if (leftPaneEntryCount < 2) return

    // Find initial cursor index so we know the click actually moved it
    const initialCursorIndex = await tauriPage.evaluate<number>(`(function() {
            var pane = document.querySelectorAll('.file-pane')[0];
            var entries = pane ? pane.querySelectorAll('.file-entry') : [];
            for (var i = 0; i < entries.length; i++) {
                if (entries[i].classList.contains('is-under-cursor')) return i;
            }
            return -1;
        })()`)

    // Pick a different entry to click — if cursor is on [0], click [1]; otherwise click [0]
    const targetIndex = initialCursorIndex === 1 ? 0 : 1

    // Dispatch mousedown then click — the cursor movement handler is on
    // onmousedown, not onclick. Must set button:0 (handleMouseDown checks it).
    await tauriPage.evaluate(`(function() {
            var pane = document.querySelectorAll('.file-pane')[0];
            var entry = pane?.querySelectorAll('.file-entry')[${targetIndex}];
            if (entry) {
                entry.scrollIntoView({block:'center'});
                var r = entry.getBoundingClientRect();
                var cx = r.left + r.width/2, cy = r.top + r.height/2;
                entry.dispatchEvent(new MouseEvent('mousedown', {bubbles:true, button:0, clientX:cx, clientY:cy}));
                entry.dispatchEvent(new MouseEvent('mouseup', {bubbles:true, button:0, clientX:cx, clientY:cy}));
                entry.dispatchEvent(new MouseEvent('click', {bubbles:true, button:0, clientX:cx, clientY:cy}));
            }
        })()`)

    // Wait for cursor to move to the clicked entry
    await pollUntil(
      tauriPage,
      async () => {
        return tauriPage.evaluate<boolean>(`(function() {
                    var pane = document.querySelectorAll('.file-pane')[0];
                    var entries = pane.querySelectorAll('.file-entry');
                    return entries[${targetIndex}]?.classList.contains('is-under-cursor') || false;
                })()`)
      },
      5000,
    )

    const entryClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[0]?.querySelectorAll('.file-entry')[${targetIndex}]?.getAttribute('class') || ''`,
    )
    expect(entryClass).toContain('is-under-cursor')
  })

  test('switches pane focus when clicking other pane', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    const paneCount = await tauriPage.count('.file-pane')
    expect(paneCount).toBe(2)

    // Click on a file entry in the right pane
    await tauriPage.evaluate(`(function() {
            var pane = document.querySelectorAll('.file-pane')[1];
            var entry = pane?.querySelector('.file-entry');
            if (entry) entry.click();
        })()`)

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

    const rightPaneClass = await tauriPage.evaluate<string>(
      `document.querySelectorAll('.file-pane')[1]?.getAttribute('class') || ''`,
    )
    expect(rightPaneClass).toContain('is-focused')

    // Click on a file entry in the left pane to transfer focus back
    await tauriPage.evaluate(`(function() {
            var pane = document.querySelectorAll('.file-pane')[0];
            var entry = pane?.querySelector('.file-entry');
            if (entry) entry.click();
        })()`)

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

test.describe('Navigation', () => {
  test('navigates into directories with Enter', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    if (!(await moveCursorToSubDir(tauriPage))) {
      test.skip()
      return
    }

    await tauriPage.keyboard.press('Enter')

    // Wait for nested-file.txt to appear
    await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(`(function() {
                    var pane = document.querySelector('.file-pane.is-focused');
                    if (!pane) return false;
                    var entries = pane.querySelectorAll('.file-entry');
                    for (var i = 0; i < entries.length; i++) {
                        if ((entries[i].querySelector('.name') || {}).textContent === 'nested-file.txt') return true;
                    }
                    return false;
                })()`),
      5000,
    )
  })

  test('navigates to parent with Backspace', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Ensure we're inside sub-dir
    const alreadyInside = await tauriPage.evaluate<boolean>(`(function() {
            var pane = document.querySelector('.file-pane.is-focused');
            if (!pane) return false;
            var entries = pane.querySelectorAll('.file-entry');
            return Array.from(entries).some(function(e) {
                return (e.querySelector('.name') || {}).textContent === 'nested-file.txt';
            });
        })()`)

    if (!alreadyInside) {
      if (!(await moveCursorToSubDir(tauriPage))) {
        test.skip()
        return
      }
      await tauriPage.keyboard.press('Enter')
      await pollUntil(
        tauriPage,
        async () =>
          tauriPage.evaluate<boolean>(`(function() {
                        var pane = document.querySelector('.file-pane.is-focused');
                        if (!pane) return false;
                        var entries = pane.querySelectorAll('.file-entry');
                        return Array.from(entries).some(function(e) {
                            return (e.querySelector('.name') || {}).textContent === 'nested-file.txt';
                        });
                    })()`),
        5000,
      )
    }

    // Press Backspace to go to parent
    await tauriPage.keyboard.press('Backspace')

    // Accept either landing: left/ (sub-dir visible) or fixture root (left visible)
    await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(`(function() {
                    var pane = document.querySelector('.file-pane.is-focused');
                    if (!pane) return false;
                    var entries = pane.querySelectorAll('.file-entry');
                    var names = Array.from(entries).map(function(e) {
                        return (e.querySelector('.name') || {}).textContent;
                    });
                    return names.indexOf('sub-dir') >= 0 || names.indexOf('left') >= 0;
                })()`),
      5000,
    )
  })
})

test.describe('New folder dialog', () => {
  test('opens new folder dialog with F7', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await tauriPage.keyboard.press('F7')

    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)

    // Verify title says "New folder"
    const titleText = await tauriPage.textContent(`${MKDIR_DIALOG} h2`)
    expect(titleText).toBe('New folder')

    // Verify subtitle contains "Create folder in"
    const subtitleText = await tauriPage.textContent(`${MKDIR_DIALOG} .subtitle`)
    expect(subtitleText).toContain('Create folder in')

    // Verify dialog has a name input
    expect(await tauriPage.isVisible(`${MKDIR_DIALOG} .name-input`)).toBe(true)

    // Verify OK and Cancel buttons
    expect(await tauriPage.isVisible(`${MKDIR_DIALOG} .btn-primary`)).toBe(true)
    expect(await tauriPage.isVisible(`${MKDIR_DIALOG} .btn-secondary`)).toBe(true)
    expect(await tauriPage.textContent(`${MKDIR_DIALOG} .btn-primary`)).toBe('OK')
    expect(await tauriPage.textContent(`${MKDIR_DIALOG} .btn-secondary`)).toBe('Cancel')

    // Close dialog with Escape
    await tauriPage.keyboard.press('Escape')

    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)
  })

  test('creates a folder and closes the dialog', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)

    const folderName = `test-folder-${Date.now()}`
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, folderName)
    await sleep(200)

    // Verify OK button is enabled
    expect(await tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`)).toBe(true)

    // Click OK
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
  })
})

test.describe('Transfer dialogs', () => {
  test('opens copy dialog with F5', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await skipParentEntry(tauriPage)

    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector('.modal-overlay', 5000)
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    const titleText = await tauriPage.textContent(`${TRANSFER_DIALOG} h2`)
    expect(titleText).toContain('Copy')

    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .path-input`)).toBe(true)

    const copyBtnText = await tauriPage.textContent(`${TRANSFER_DIALOG} .btn-primary`)
    expect(copyBtnText).toBe('Copy')
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .btn-secondary`)).toBe(true)

    await tauriPage.keyboard.press('Escape')

    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)
    expect(await tauriPage.isVisible('.modal-overlay')).toBe(false)
  })

  test('opens move dialog with F6', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await skipParentEntry(tauriPage)

    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector('.modal-overlay', 5000)
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    const titleText = await tauriPage.textContent(`${TRANSFER_DIALOG} h2`)
    expect(titleText).toContain('Move')
    expect(titleText).not.toContain('Copy')

    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .path-input`)).toBe(true)

    const moveBtnText = await tauriPage.textContent(`${TRANSFER_DIALOG} .btn-primary`)
    expect(moveBtnText).toBe('Move')
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .btn-secondary`)).toBe(true)

    await tauriPage.keyboard.press('Escape')

    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)
    expect(await tauriPage.isVisible('.modal-overlay')).toBe(false)
  })
})
