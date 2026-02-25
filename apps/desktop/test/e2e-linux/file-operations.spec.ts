/**
 * E2E tests for file operations in the Cmdr Tauri application on Linux.
 *
 * These tests verify file operation round-trips: the UI action is performed,
 * then both DOM state and on-disk state are verified. Fixtures are recreated
 * before each test via wdio.conf.ts `beforeTest`.
 *
 * Fixture layout (at $CMDR_E2E_START_PATH):
 *   left/                        <- left pane starts here
 *     file-a.txt, file-b.txt, sub-dir/, bulk/, .hidden-file
 *   right/                       <- right pane starts here (empty)
 *
 * WebKitGTK WebDriver quirks addressed in these tests:
 * - Native WebDriver clicks fail on non-form elements -- use jsClick()
 * - browser.keys(' ') doesn't deliver Space -- use pressSpaceKey()
 */

import fs from 'fs'
import path from 'path'
import {
    ensureAppReady,
    getEntryName,
    fileExistsInFocusedPane,
    fileExistsInPane,
    findFileIndex,
    MKDIR_DIALOG,
    TRANSFER_DIALOG,
} from '../e2e-shared/helpers.js'

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Clicks an element via JavaScript, bypassing WebKitGTK WebDriver's strict
 * clickability checks that reject clicks on non-form elements.
 */
async function jsClick(element: WebdriverIO.Element): Promise<void> {
    await browser.execute((el: HTMLElement) => el.click(), element as unknown as HTMLElement)
}

/**
 * Moves the cursor to a specific file by name in the focused pane.
 * Uses findFileIndex() for DOM reading, then navigates with keyboard
 * to preserve WebDriver focus.
 */
async function moveCursorToFile(targetName: string): Promise<boolean> {
    const info = await findFileIndex(targetName)

    if ('error' in info || info.targetIndex < 0) {
        return false
    }

    // Navigate: Home to reset to first entry, then ArrowDown to target
    await browser.keys('Home')
    await browser.pause(100)
    for (let i = 0; i < info.targetIndex; i++) {
        await browser.keys('ArrowDown')
        await browser.pause(50)
    }
    await browser.pause(100)
    return true
}

/** Returns the fixture root path from the environment variable. */
function getFixtureRoot(): string {
    const root = process.env.CMDR_E2E_START_PATH
    if (!root) throw new Error('CMDR_E2E_START_PATH env var is not set')
    return root
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('Copy round-trip', () => {
    it('copies file-a.txt from left pane to right pane via F5', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        // Move cursor to file-a.txt
        const found = await moveCursorToFile('file-a.txt')
        expect(found).toBe(true)

        // Press F5 to open copy dialog
        await browser.keys('F5')

        // Wait for transfer dialog to appear
        const dialog = browser.$(TRANSFER_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Verify title contains "Copy"
        const title = browser.$(`${TRANSFER_DIALOG} h2`)
        expect(await title.getText()).toContain('Copy')

        // Click the Copy button to confirm
        const copyButton = browser.$(`${TRANSFER_DIALOG} button.primary`) as unknown as WebdriverIO.Element
        await jsClick(copyButton)

        // Wait for dialog to close (confirms copy succeeded)
        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 10000, reverse: true })

        // Switch to right pane to verify the file appeared in DOM
        await browser.keys('Tab')
        await browser.pause(500)

        await browser.waitUntil(
            async () => fileExistsInFocusedPane('file-a.txt'),
            { timeout: 5000, timeoutMsg: 'file-a.txt did not appear in right pane after copy' },
        )

        // Verify on disk: file exists in right dir
        expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt'))).toBe(true)

        // Verify original still exists in left dir
        expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
    })
})

describe('Move round-trip', () => {
    it('moves file-b.txt from left pane to right pane via F6', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        // Move cursor to file-b.txt
        const found = await moveCursorToFile('file-b.txt')
        expect(found).toBe(true)

        // Press F6 to open move dialog
        await browser.keys('F6')

        // Wait for transfer dialog to appear
        const dialog = browser.$(TRANSFER_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Verify title contains "Move"
        const title = browser.$(`${TRANSFER_DIALOG} h2`)
        expect(await title.getText()).toContain('Move')

        // Click the Move button to confirm
        const moveButton = browser.$(`${TRANSFER_DIALOG} button.primary`) as unknown as WebdriverIO.Element
        await jsClick(moveButton)

        // Wait for dialog to close (confirms move succeeded)
        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 10000, reverse: true })
        await browser.pause(500)

        // Verify file-b.txt is gone from left pane DOM
        const goneFromLeft = await browser.waitUntil(
            async () => !(await fileExistsInPane('file-b.txt', 0)),
            { timeout: 5000, timeoutMsg: 'file-b.txt did not disappear from left pane after move' },
        )
        expect(goneFromLeft).toBe(true)

        // Switch to right pane and verify file-b.txt appeared
        await browser.keys('Tab')
        await browser.pause(500)

        await browser.waitUntil(
            async () => fileExistsInFocusedPane('file-b.txt'),
            { timeout: 5000, timeoutMsg: 'file-b.txt did not appear in right pane after move' },
        )

        // Verify on disk: file is gone from left, present in right
        expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-b.txt'))).toBe(false)
        expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-b.txt'))).toBe(true)
    })
})

describe('Rename round-trip', () => {
    it('renames file-a.txt to renamed-file.txt via F2', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        // Move cursor to file-a.txt
        const found = await moveCursorToFile('file-a.txt')
        expect(found).toBe(true)

        // Press F2 to activate inline rename
        await browser.keys('F2')

        // Wait for the inline rename input to appear
        const renameInput = browser.$('.rename-input')
        await renameInput.waitForExist({ timeout: 5000 })

        // Clear existing value and type new name
        // The input pre-selects the filename (without extension), so typing replaces it
        await renameInput.setValue('renamed-file.txt')
        await browser.pause(200)

        // Press Enter to confirm rename
        await browser.keys('Enter')

        // Wait for the rename input to disappear (confirms rename completed)
        await browser.waitUntil(
            async () => !(await renameInput.isExisting()),
            { timeout: 5000, timeoutMsg: 'Rename input did not close after Enter' },
        )
        await browser.pause(500)

        // Verify new name appears in pane DOM
        await browser.waitUntil(
            async () => fileExistsInFocusedPane('renamed-file.txt'),
            { timeout: 5000, timeoutMsg: 'renamed-file.txt did not appear in listing after rename' },
        )

        // Verify old name is gone from pane DOM
        const oldNameGone = !(await fileExistsInFocusedPane('file-a.txt'))
        expect(oldNameGone).toBe(true)

        // Verify on disk
        expect(fs.existsSync(path.join(fixtureRoot, 'left', 'renamed-file.txt'))).toBe(true)
        expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(false)
    })
})

describe('Create folder round-trip', () => {
    it('creates a new folder via F7 and verifies on disk', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        const folderName = `new-test-folder-${Date.now()}`

        // Press F7 to open new folder dialog
        await browser.keys('F7')

        // Wait for dialog to appear
        const dialog = browser.$(MKDIR_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Type folder name
        const nameInput = browser.$(`${MKDIR_DIALOG} .name-input`)
        await nameInput.waitForExist({ timeout: 3000 })
        await nameInput.setValue(folderName)
        await browser.pause(200)

        // Click OK to create
        const okButton = browser.$(`${MKDIR_DIALOG} button.primary`) as unknown as WebdriverIO.Element
        await jsClick(okButton)

        // Wait for dialog to close
        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 5000, reverse: true })
        await browser.pause(500)

        // Verify folder appears in listing
        await browser.waitUntil(
            async () => fileExistsInFocusedPane(folderName),
            { timeout: 5000, timeoutMsg: `${folderName} did not appear in listing after creation` },
        )

        // Verify on disk
        const folderPath = path.join(fixtureRoot, 'left', folderName)
        expect(fs.existsSync(folderPath)).toBe(true)
        expect(fs.statSync(folderPath).isDirectory()).toBe(true)
    })
})

describe('View mode toggle', () => {
    it('switches between Brief and Full view modes', async () => {
        await ensureAppReady()

        // Determine current view mode by checking which container class exists
        const hasBriefList = await browser.$('.brief-list-container').isExisting()
        const hasFullList = await browser.$('.full-list-container').isExisting()

        // At least one view mode should be active
        expect(hasBriefList || hasFullList).toBe(true)

        if (hasBriefList) {
            // Currently in Brief mode -- switch to Full view via command palette
            // On Linux with WebKitGTK, Tauri maps Cmd to Ctrl
            // Use the Tauri event system to trigger view mode change
            await browser.execute(() => {
                // Dispatch a custom event that the app listens for
                document.dispatchEvent(
                    new KeyboardEvent('keydown', {
                        key: '1',
                        metaKey: true,
                        bubbles: true,
                    }),
                )
            })
            await browser.pause(500)

            // Verify Full view mode is now active
            await browser.waitUntil(
                async () => browser.$('.full-list-container').isExisting(),
                { timeout: 5000, timeoutMsg: 'Full list container did not appear after switching view mode' },
            )

            // Full mode should have a header row with column headers
            const headerRow = browser.$('.full-list-container .header-row')
            expect(await headerRow.isExisting()).toBe(true)

            // Switch back to Brief view
            await browser.execute(() => {
                document.dispatchEvent(
                    new KeyboardEvent('keydown', {
                        key: '2',
                        metaKey: true,
                        bubbles: true,
                    }),
                )
            })
            await browser.pause(500)

            await browser.waitUntil(
                async () => browser.$('.brief-list-container').isExisting(),
                { timeout: 5000, timeoutMsg: 'Brief list container did not reappear after switching back' },
            )
        } else {
            // Currently in Full mode -- switch to Brief view
            await browser.execute(() => {
                document.dispatchEvent(
                    new KeyboardEvent('keydown', {
                        key: '2',
                        metaKey: true,
                        bubbles: true,
                    }),
                )
            })
            await browser.pause(500)

            await browser.waitUntil(
                async () => browser.$('.brief-list-container').isExisting(),
                { timeout: 5000, timeoutMsg: 'Brief list container did not appear after switching view mode' },
            )

            // Brief mode should have a header row
            const headerRow = browser.$('.brief-list-container .header-row')
            expect(await headerRow.isExisting()).toBe(true)

            // Switch back to Full view
            await browser.execute(() => {
                document.dispatchEvent(
                    new KeyboardEvent('keydown', {
                        key: '1',
                        metaKey: true,
                        bubbles: true,
                    }),
                )
            })
            await browser.pause(500)

            await browser.waitUntil(
                async () => browser.$('.full-list-container').isExisting(),
                { timeout: 5000, timeoutMsg: 'Full list container did not reappear after switching back' },
            )
        }
    })
})

describe('Hidden files toggle', () => {
    it('toggles hidden file visibility', async () => {
        await ensureAppReady()

        // Check initial state: .hidden-file may or may not be visible depending on default
        const initiallyVisible = await fileExistsInFocusedPane('.hidden-file')

        // Toggle hidden files via simulated keyboard shortcut (Cmd+Shift+.)
        // On Linux/WebKitGTK, metaKey maps to Ctrl
        await browser.execute(() => {
            document.dispatchEvent(
                new KeyboardEvent('keydown', {
                    key: '.',
                    metaKey: true,
                    shiftKey: true,
                    bubbles: true,
                }),
            )
        })
        await browser.pause(500)

        // After toggle, visibility should be inverted
        const afterToggleVisible = await fileExistsInFocusedPane('.hidden-file')
        expect(afterToggleVisible).not.toBe(initiallyVisible)

        // Toggle again to restore original state
        await browser.execute(() => {
            document.dispatchEvent(
                new KeyboardEvent('keydown', {
                    key: '.',
                    metaKey: true,
                    shiftKey: true,
                    bubbles: true,
                }),
            )
        })
        await browser.pause(500)

        // Should be back to original state
        const afterSecondToggle = await fileExistsInFocusedPane('.hidden-file')
        expect(afterSecondToggle).toBe(initiallyVisible)
    })
})

describe('Command palette', () => {
    it('opens, shows results, and closes with Escape', async () => {
        await ensureAppReady()

        // Open command palette via keyboard shortcut (Cmd+Shift+P)
        await browser.execute(() => {
            document.dispatchEvent(
                new KeyboardEvent('keydown', {
                    key: 'p',
                    metaKey: true,
                    shiftKey: true,
                    bubbles: true,
                }),
            )
        })

        // Wait for command palette overlay to appear
        const paletteOverlay = browser.$('.palette-overlay')
        await paletteOverlay.waitForExist({ timeout: 5000 })

        // Verify the search input exists and is focused
        const searchInput = browser.$('.palette-overlay .search-input')
        expect(await searchInput.isExisting()).toBe(true)

        // Verify results are shown (palette shows all commands when empty query)
        const resultsContainer = browser.$('.palette-overlay .results-container')
        expect(await resultsContainer.isExisting()).toBe(true)

        // Type a query to filter results
        await searchInput.setValue('sort')
        await browser.pause(300)

        // Verify filtered results contain matching items
        const resultItems = await browser.$$('.palette-overlay .result-item')
        expect(resultItems.length).toBeGreaterThan(0)

        // Verify at least one result contains "sort" in its text
        let hasSortResult = false
        for (const item of resultItems) {
            const text = await item.getText()
            if (text.toLowerCase().includes('sort')) {
                hasSortResult = true
                break
            }
        }
        expect(hasSortResult).toBe(true)

        // Close palette with Escape
        await browser.keys('Escape')

        // Wait for palette to disappear
        await browser.waitUntil(
            async () => !(await paletteOverlay.isExisting()),
            { timeout: 3000, timeoutMsg: 'Command palette did not close after Escape' },
        )
    })
})

describe('Empty directory', () => {
    it('shows empty right pane gracefully without crash', async () => {
        await ensureAppReady()

        // Switch to right pane (which starts empty)
        await browser.keys('Tab')
        await browser.pause(500)

        // Verify right pane is focused
        const panes = await browser.$$('.file-pane')
        const rightPaneClass = await panes[1].getAttribute('class')
        expect(rightPaneClass).toContain('is-focused')

        // The pane should render without errors. At minimum it should have
        // the ".." parent entry or show an empty state.
        const entries = [...(await panes[1].$$('.file-entry'))]

        if (entries.length > 0) {
            // If there are entries, the first one should be ".."
            const firstEntryName = await getEntryName(entries[0])
            expect(firstEntryName).toBe('..')
        }

        // Verify no error message is shown
        const errorMessage = panes[1].$('.error-message')
        expect(await errorMessage.isExisting()).toBe(false)

        // Verify the pane still renders (not crashed) by checking structure
        const filePane = browser.$('.file-pane.is-focused')
        expect(await filePane.isExisting()).toBe(true)

        // Can still switch back to left pane without issues
        await browser.keys('Tab')
        await browser.pause(300)

        const leftPaneClass = await panes[0].getAttribute('class')
        expect(leftPaneClass).toContain('is-focused')
    })
})
