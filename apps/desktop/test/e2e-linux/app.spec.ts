/**
 * E2E tests for the Cmdr Tauri application on Linux.
 *
 * These tests run against the actual built Tauri app using tauri-driver
 * and WebDriverIO. They verify real application behavior including
 * frontend-backend integration with file operations.
 *
 * Note: On Linux, some features use stubs (volumes, network) so tests
 * are adapted to work with the stubbed implementations.
 *
 * Usage:
 *   pnpm test:e2e:linux        # Run via Docker (recommended)
 *   pnpm test:e2e:linux:native # Run natively on Linux
 */

/**
 * Helper to get file entry name text. Works with both Full and Brief view modes.
 * In Full mode, the name is in `.col-name`. In Brief mode, it's in `.name`.
 */
async function getEntryName(entry: WebdriverIO.Element): Promise<string> {
    // Try Full view mode selector first
    const colName = await entry.$('.col-name')
    if (await colName.isExisting()) {
        return colName.getText()
    }
    // Fall back to Brief view mode selector
    const name = await entry.$('.name')
    if (await name.isExisting()) {
        return name.getText()
    }
    // Last resort: get all text from the entry
    return entry.getText()
}

/**
 * Helper to ensure app is ready and panes have focus initialized.
 */
async function ensureAppReady(): Promise<void> {
    const fileEntry = await browser.$('.file-entry')
    await fileEntry.waitForExist({ timeout: 10000 })

    // Click on left pane to ensure it has focus
    const leftPane = await browser.$('.file-pane')
    await leftPane.click()
    await browser.pause(300)
}

describe('Basic rendering', () => {
    it('should launch and show the main window', async () => {
        // Wait for the app to fully load
        await browser.pause(3000)

        // Get the window title
        const title = await browser.getTitle()
        expect(title).toContain('Cmdr')
    })

    it('should display the dual pane interface', async () => {
        // Wait for the dual pane explorer to render
        const explorer = await browser.$('.dual-pane-explorer')
        await explorer.waitForExist({ timeout: 10000 })

        // Verify both panes are present
        const panes = await browser.$$('.file-pane')
        expect(panes.length).toBe(2)
    })

    it('should show file entries in the panes', async () => {
        // Wait for file entries to load
        const fileEntry = await browser.$('.file-entry')
        await fileEntry.waitForExist({ timeout: 10000 })

        // Should have file entries
        const fileEntries = await browser.$$('.file-entry')
        expect(fileEntries.length).toBeGreaterThan(0)
    })
})

describe('Keyboard navigation', () => {
    it('should move cursor with arrow keys', async () => {
        await ensureAppReady()

        // Get all file entries and find which one has the cursor
        const entries = await browser.$$('.file-entry')
        if (entries.length < 2) {
            // Not enough entries to test cursor movement
            console.log('Skipping arrow key test: fewer than 2 entries')
            return
        }

        // Find initial cursor position by index
        let initialCursorIndex = -1
        for (let i = 0; i < entries.length; i++) {
            const cls = await entries[i].getAttribute('class')
            if (cls.includes('is-under-cursor')) {
                initialCursorIndex = i
                break
            }
        }

        expect(initialCursorIndex).toBeGreaterThanOrEqual(0)

        // Press ArrowDown to move cursor
        await browser.keys('ArrowDown')
        await browser.pause(300)

        // Re-query entries and find new cursor position
        const updatedEntries = await browser.$$('.file-entry')
        let newCursorIndex = -1
        for (let i = 0; i < updatedEntries.length; i++) {
            const cls = await updatedEntries[i].getAttribute('class')
            if (cls.includes('is-under-cursor')) {
                newCursorIndex = i
                break
            }
        }

        expect(newCursorIndex).toBeGreaterThanOrEqual(0)

        // Cursor should have moved (wraps if at end)
        expect(newCursorIndex).not.toBe(initialCursorIndex)
    })

    it('should switch panes with Tab key', async () => {
        await ensureAppReady()

        // Re-query panes after ensureAppReady
        let panes = await browser.$$('.file-pane')
        expect(panes.length).toBe(2)

        // Verify left pane is focused (ensureAppReady clicked on it)
        const leftPaneClass = await panes[0].getAttribute('class')
        expect(leftPaneClass).toContain('is-focused')

        // Press Tab to switch to right pane
        await browser.keys('Tab')
        await browser.pause(300)

        // Re-query panes (DOM may have updated)
        panes = await browser.$$('.file-pane')

        // Check right pane is now focused
        const rightPaneClass = await panes[1].getAttribute('class')
        expect(rightPaneClass).toContain('is-focused')

        // Left pane should not be focused
        const leftPaneClassAfter = await panes[0].getAttribute('class')
        expect(leftPaneClassAfter).not.toContain('is-focused')

        // Press Tab again to go back to left pane
        await browser.keys('Tab')
        await browser.pause(300)

        panes = await browser.$$('.file-pane')
        const leftPaneClassFinal = await panes[0].getAttribute('class')
        expect(leftPaneClassFinal).toContain('is-focused')
    })

    it('should toggle selection with Space key', async () => {
        await ensureAppReady()

        // Get cursor entry
        let cursorEntry = await browser.$('.file-entry.is-under-cursor')
        const cursorText = await getEntryName(cursorEntry)

        // Skip ".." entry if that's where cursor is
        if (cursorText === '..') {
            await browser.keys('ArrowDown')
            await browser.pause(300)
            cursorEntry = await browser.$('.file-entry.is-under-cursor')
        }

        // Verify not selected initially
        let cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).not.toContain('is-selected')

        // Press Space to select
        await browser.keys('Space')
        await browser.pause(300)

        // Verify now selected - re-query the cursor entry
        cursorEntry = await browser.$('.file-entry.is-under-cursor')
        cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).toContain('is-selected')

        // Press Space again to deselect
        await browser.keys('Space')
        await browser.pause(300)

        // Verify now deselected
        cursorEntry = await browser.$('.file-entry.is-under-cursor')
        cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).not.toContain('is-selected')
    })
})

describe('Mouse interactions', () => {
    it('should move cursor when clicking a file entry', async () => {
        await ensureAppReady()

        const entries = await browser.$$('.file-entry')
        if (entries.length < 2) {
            // Skip if not enough entries
            return
        }

        // Click on second entry
        await entries[1].click()
        await browser.pause(300)

        // Re-query and verify cursor moved to clicked entry
        const updatedEntries = await browser.$$('.file-entry')
        const entryClass = await updatedEntries[1].getAttribute('class')
        expect(entryClass).toContain('is-under-cursor')
    })

    it('should switch pane focus when clicking other pane', async () => {
        await ensureAppReady()

        let panes = await browser.$$('.file-pane')
        expect(panes.length).toBe(2)

        // Click on right pane
        await panes[1].click()
        await browser.pause(300)

        // Re-query and verify right pane is focused
        panes = await browser.$$('.file-pane')
        const rightPaneClass = await panes[1].getAttribute('class')
        expect(rightPaneClass).toContain('is-focused')

        // Click on left pane
        await panes[0].click()
        await browser.pause(300)

        // Re-query and verify left pane is focused
        panes = await browser.$$('.file-pane')
        const leftPaneClass = await panes[0].getAttribute('class')
        expect(leftPaneClass).toContain('is-focused')
    })
})

describe('Navigation', () => {
    it('should navigate into directories with Enter', async () => {
        await ensureAppReady()

        // Get current path from the focused pane's header
        let pathElement = await browser.$('.file-pane.is-focused .header .path')
        const initialPath = await pathElement.getText()

        // Find a directory entry (has .size-dir class which shows "<dir>")
        const dirEntry = await browser.$('.file-entry:has(.size-dir)')

        let hasNavigableEntry = false

        if (await dirEntry.isExisting()) {
            await dirEntry.click()
            hasNavigableEntry = true
        }

        if (!hasNavigableEntry) {
            // No directories to navigate into, skip test
            console.log('Skipping navigation test: no directories found')
            return
        }

        // Press Enter to navigate
        await browser.keys('Enter')
        await browser.pause(1000)

        // Re-query and verify path changed
        pathElement = await browser.$('.file-pane.is-focused .header .path')
        const newPath = await pathElement.getText()
        expect(newPath).not.toBe(initialPath)
    })

    it('should navigate to parent with Backspace', async () => {
        await ensureAppReady()

        // First, navigate into a directory so we can go back
        const dirEntry = await browser.$('.file-entry:has(.size-dir)')

        if (!(await dirEntry.isExisting())) {
            // No directories, skip test
            console.log('Skipping backspace test: no directories found')
            return
        }

        // Navigate into a directory first
        await dirEntry.click()
        await browser.keys('Enter')
        await browser.pause(1000)

        // Get current path
        let pathElement = await browser.$('.file-pane.is-focused .header .path')
        const currentPath = await pathElement.getText()

        // Press Backspace to go to parent
        await browser.keys('Backspace')
        await browser.pause(1000)

        // Re-query and verify path changed
        pathElement = await browser.$('.file-pane.is-focused .header .path')
        const newPath = await pathElement.getText()
        expect(newPath).not.toBe(currentPath)
    })
})

describe('New folder dialog', () => {
    it('should open new folder dialog with F7', async () => {
        await ensureAppReady()

        // Press F7 to open new folder dialog
        await browser.keys('F7')

        // Wait for modal overlay to appear
        const modalOverlay = await browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 5000 })

        // Verify new folder dialog appears
        const dialog = await browser.$('.new-folder-dialog')
        await dialog.waitForExist({ timeout: 5000 })

        // Verify title says "New folder"
        const title = await browser.$('.new-folder-dialog h2')
        expect(await title.getText()).toBe('New folder')

        // Verify subtitle contains "Create folder in"
        const subtitle = await browser.$('.new-folder-dialog .subtitle')
        const subtitleText = await subtitle.getText()
        expect(subtitleText).toContain('Create folder in')

        // Verify dialog has a name input
        const nameInput = await browser.$('.new-folder-dialog .name-input')
        expect(await nameInput.isExisting()).toBe(true)

        // Verify OK and Cancel buttons exist
        const okButton = await browser.$('.new-folder-dialog button.primary')
        const cancelButton = await browser.$('.new-folder-dialog button.secondary')
        expect(await okButton.isExisting()).toBe(true)
        expect(await cancelButton.isExisting()).toBe(true)
        expect(await okButton.getText()).toBe('OK')
        expect(await cancelButton.getText()).toBe('Cancel')

        // Close dialog with Escape
        await browser.keys('Escape')

        // Wait for dialog to close
        await modalOverlay.waitForExist({ timeout: 3000, reverse: true })
    })

    it('should create a folder and close the dialog', async () => {
        await ensureAppReady()

        // Press F7 to open new folder dialog
        await browser.keys('F7')

        // Wait for dialog to appear
        const dialog = await browser.$('.new-folder-dialog')
        await dialog.waitForExist({ timeout: 5000 })

        // Type a unique folder name
        const folderName = `test-folder-${Date.now()}`
        const nameInput = await browser.$('.new-folder-dialog .name-input')
        await nameInput.waitForExist({ timeout: 3000 })
        await nameInput.setValue(folderName)
        await browser.pause(200)

        // Verify OK button is enabled
        const okButton = await browser.$('.new-folder-dialog button.primary')
        expect(await okButton.isEnabled()).toBe(true)

        // Click OK to create the folder
        await okButton.click()

        // Wait for dialog to close (confirms create_directory succeeded)
        const modalOverlay = await browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 5000, reverse: true })
    })
})

describe('Copy dialog', () => {
    it('should open copy dialog with F5', async () => {
        await ensureAppReady()

        // Move cursor to a file (skip ".." entry)
        const cursorEntry = await browser.$('.file-entry.is-under-cursor')
        const cursorText = await getEntryName(cursorEntry)

        if (cursorText === '..') {
            await browser.keys('ArrowDown')
            await browser.pause(300)
        }

        // Press F5 to open copy dialog
        await browser.keys('F5')
        await browser.pause(500)

        // Verify copy dialog appears
        const modalOverlay = await browser.$('.modal-overlay')
        expect(await modalOverlay.isExisting()).toBe(true)

        const copyDialog = await browser.$('.copy-dialog')
        expect(await copyDialog.isExisting()).toBe(true)

        // Verify dialog has path input
        const pathInput = await browser.$('.copy-dialog .path-input')
        expect(await pathInput.isExisting()).toBe(true)

        // Verify dialog has Copy and Cancel buttons (using class selectors)
        // Copy button has class "primary", Cancel button has class "secondary"
        const copyButton = await browser.$('.copy-dialog button.primary')
        const cancelButton = await browser.$('.copy-dialog button.secondary')
        expect(await copyButton.isExisting()).toBe(true)
        expect(await cancelButton.isExisting()).toBe(true)

        // Close dialog with Escape
        await browser.keys('Escape')
        await browser.pause(300)

        // Verify dialog is closed
        const modalAfter = await browser.$('.modal-overlay')
        expect(await modalAfter.isExisting()).toBe(false)
    })
})
