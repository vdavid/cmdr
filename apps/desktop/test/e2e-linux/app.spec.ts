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
 * WebKitGTK WebDriver quirks addressed in these tests:
 * - Native WebDriver clicks fail on non-form elements — use jsClick()
 * - browser.keys(' ') doesn't deliver Space — use pressSpaceKey()
 *
 * Usage:
 *   pnpm test:e2e:linux        # Run via Docker (recommended)
 *   pnpm test:e2e:linux:native # Run natively on Linux
 */

// ─── Helpers ────────────────────────────────────────────────────────────────

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
 * Clicks on a file entry (not the container pane) because WebKitGTK's
 * WebDriver rejects clicks on non-interactive container elements.
 */
async function ensureAppReadyWithFocus(): Promise<void> {
    // Wait for file entries to be visible (confirms app is fully loaded)
    const fileEntry = await browser.$('.file-entry')
    await fileEntry.waitForDisplayed({ timeout: 10000 })

    // Wait for the HTML loading screen to be gone
    const loadingScreen = await browser.$('#loading-screen')
    if (await loadingScreen.isExisting()) {
        await browser.waitUntil(async () => !(await loadingScreen.isDisplayed()), {
            timeout: 5000,
            timeoutMsg: 'Loading screen did not disappear',
        })
    }

    // Dismiss any overlays (AI notification, etc.) via JS click to bypass
    // WebKitGTK's strict clickability checks
    await browser.execute(() => {
        const dismissBtn = document.querySelector('.ai-notification .ai-button.secondary') as HTMLElement | null
        dismissBtn?.click()
    })
    await browser.pause(300)

    // Click on a file entry in the left pane to ensure focus, then
    // focus the explorer container so keyboard events reach the handler.
    await browser.execute(() => {
        const entry = document.querySelector('.file-pane .file-entry') as HTMLElement | null
        entry?.click()
        const explorer = document.querySelector('.dual-pane-explorer') as HTMLElement | null
        explorer?.focus()
    })
    await browser.pause(300)
}

/**
 * Clicks an element via JavaScript, bypassing WebKitGTK WebDriver's strict
 * clickability checks that reject clicks on non-form elements.
 */
async function jsClick(element: WebdriverIO.Element): Promise<void> {
    await browser.execute((el: HTMLElement) => el.click(), element as unknown as HTMLElement)
}

/**
 * Sends a Space key event via the W3C Actions API.
 * browser.keys(' ') doesn't deliver Space in WebKitGTK WebDriver due to how
 * it handles the CharKey vs VirtualKey code paths for the space character.
 * The explicit key down/up via the Actions API works around this.
 * See: https://github.com/webdriverio/webdriverio/issues/10996
 * See: https://github.com/SeleniumHQ/selenium/issues/4334
 */
async function pressSpaceKey(): Promise<void> {
    await browser.action('key').down(' ').pause(50).up(' ').perform()
    await browser.releaseActions()
    await browser.pause(300)
}

// ─── Selectors ───────────────────────────────────────────────────────────────

// ModalDialog renders as .modal-overlay[data-dialog-id] > .modal-dialog,
// with no dialog-specific CSS class. Use data-dialog-id to target each dialog.
const MKDIR_DIALOG = '[data-dialog-id="mkdir-confirmation"]'
const COPY_DIALOG = '[data-dialog-id="copy-confirmation"]'

// ─── Tests ──────────────────────────────────────────────────────────────────

describe('Basic rendering', () => {
    it('launches and shows the main window', async () => {
        // Wait for the app to fully load
        await browser.pause(3000)

        // Get the window title
        const title = await browser.getTitle()
        expect(title).toContain('Cmdr')
    })

    it('displays the dual pane interface', async () => {
        // Wait for the dual pane explorer to render
        const explorer = await browser.$('.dual-pane-explorer')
        await explorer.waitForExist({ timeout: 10000 })

        // Verify both panes are present
        const panes = await browser.$$('.file-pane')
        expect(panes.length).toBe(2)
    })

    it('shows file entries in the panes', async () => {
        // Wait for file entries to load
        const fileEntry = await browser.$('.file-entry')
        await fileEntry.waitForExist({ timeout: 10000 })

        // Should have file entries
        const fileEntries = await browser.$$('.file-entry')
        expect(fileEntries.length).toBeGreaterThan(0)
    })
})

describe('Keyboard navigation', () => {
    it('moves cursor with arrow keys', async () => {
        await ensureAppReadyWithFocus()

        // Get all file entries and find which one has the cursor
        // Spread to convert ChainablePromiseArray to real array for .length
        const entries = [...(await browser.$$('.file-entry'))]
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
        const updatedEntries = [...(await browser.$$('.file-entry'))]
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

    it('switches panes with Tab key', async () => {
        await ensureAppReadyWithFocus()

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

    it('toggles selection with Space key', async () => {
        await ensureAppReadyWithFocus()

        // Get cursor entry (cast needed due to WDIO ChainablePromiseElement type quirk)
        let cursorEntry = (await browser.$('.file-entry.is-under-cursor')) as unknown as WebdriverIO.Element
        const cursorText = await getEntryName(cursorEntry)

        // Skip ".." entry if that's where cursor is
        if (cursorText === '..') {
            await browser.keys('ArrowDown')
            await browser.pause(300)
            cursorEntry = (await browser.$('.file-entry.is-under-cursor')) as unknown as WebdriverIO.Element
        }

        // Verify not selected initially
        let cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).not.toContain('is-selected')

        // Press Space to select — uses W3C Actions API with explicit key down/up
        // because browser.keys(' ') doesn't deliver Space in WebKitGTK WebDriver
        await pressSpaceKey()

        // Verify now selected - re-query the cursor entry
        cursorEntry = (await browser.$('.file-entry.is-under-cursor')) as unknown as WebdriverIO.Element
        cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).toContain('is-selected')

        // Press Space again to deselect
        await pressSpaceKey()

        // Verify now deselected
        cursorEntry = (await browser.$('.file-entry.is-under-cursor')) as unknown as WebdriverIO.Element
        cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).not.toContain('is-selected')
    })
})

describe('Mouse interactions', () => {
    it('moves cursor when clicking a file entry', async () => {
        await ensureAppReadyWithFocus()

        // Scope to left pane only ($$('.file-entry') returns entries from both panes)
        const panes = [...(await browser.$$('.file-pane'))]
        const entries = [...(await panes[0].$$('.file-entry'))]
        if (entries.length < 2) {
            // Skip if not enough entries
            return
        }

        // Click on second entry via JS (WebKitGTK rejects native clicks on non-form elements)
        await jsClick(entries[1])
        await browser.pause(300)

        // Re-query and verify cursor moved to clicked entry
        const updatedEntries = [...(await panes[0].$$('.file-entry'))]
        const entryClass = await updatedEntries[1].getAttribute('class')
        expect(entryClass).toContain('is-under-cursor')
    })

    it('switches pane focus when clicking other pane', async () => {
        await ensureAppReadyWithFocus()

        let panes = [...(await browser.$$('.file-pane'))]
        expect(panes.length).toBe(2)

        // Click on a file entry in the right pane to transfer focus
        const rightPaneEntry = (await panes[1].$('.file-entry')) as unknown as WebdriverIO.Element
        await jsClick(rightPaneEntry)
        await browser.pause(300)

        // Re-query and verify right pane is focused
        panes = [...(await browser.$$('.file-pane'))]
        const rightPaneClass = await panes[1].getAttribute('class')
        expect(rightPaneClass).toContain('is-focused')

        // Click on a file entry in the left pane to transfer focus back
        const leftPaneEntry = (await panes[0].$('.file-entry')) as unknown as WebdriverIO.Element
        await jsClick(leftPaneEntry)
        await browser.pause(300)

        // Re-query and verify left pane is focused
        panes = [...(await browser.$$('.file-pane'))]
        const leftPaneClass = await panes[0].getAttribute('class')
        expect(leftPaneClass).toContain('is-focused')
    })
})

describe('Navigation', () => {
    it('navigates into directories with Enter', async () => {
        await ensureAppReadyWithFocus()

        // Get current path from the focused pane's header
        let pathElement = await browser.$('.file-pane.is-focused .header .path')
        const initialPath = await pathElement.getText()

        // Find a directory entry (has .size-dir class which shows "<dir>")
        const dirEntry = (await browser.$(
            '.file-pane.is-focused .file-entry:has(.size-dir)',
        )) as unknown as WebdriverIO.Element

        if (!(await dirEntry.isExisting())) {
            // No directories to navigate into, skip test
            console.log('Skipping navigation test: no directories found')
            return
        }

        await jsClick(dirEntry)
        await browser.pause(300)

        // Press Enter to navigate
        await browser.keys('Enter')
        await browser.pause(1000)

        // Re-query and verify path changed
        pathElement = await browser.$('.file-pane.is-focused .header .path')
        const newPath = await pathElement.getText()
        expect(newPath).not.toBe(initialPath)
    })

    it('navigates to parent with Backspace', async () => {
        await ensureAppReadyWithFocus()

        // First, navigate into a directory so we can go back
        const dirEntry = (await browser.$(
            '.file-pane.is-focused .file-entry:has(.size-dir)',
        )) as unknown as WebdriverIO.Element

        if (!(await dirEntry.isExisting())) {
            // No directories, skip test
            console.log('Skipping backspace test: no directories found')
            return
        }

        // Navigate into a directory first
        await jsClick(dirEntry)
        await browser.pause(300)
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
    it('opens new folder dialog with F7', async () => {
        await ensureAppReadyWithFocus()

        // Press F7 to open new folder dialog
        await browser.keys('F7')

        // Verify new folder dialog appears
        const dialog = await browser.$(MKDIR_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Verify title says "New folder"
        const title = await browser.$(`${MKDIR_DIALOG} h2`)
        expect(await title.getText()).toBe('New folder')

        // Verify subtitle contains "Create folder in"
        const subtitle = await browser.$(`${MKDIR_DIALOG} .subtitle`)
        const subtitleText = await subtitle.getText()
        expect(subtitleText).toContain('Create folder in')

        // Verify dialog has a name input
        const nameInput = await browser.$(`${MKDIR_DIALOG} .name-input`)
        expect(await nameInput.isExisting()).toBe(true)

        // Verify OK and Cancel buttons exist
        const okButton = await browser.$(`${MKDIR_DIALOG} button.primary`)
        const cancelButton = await browser.$(`${MKDIR_DIALOG} button.secondary`)
        expect(await okButton.isExisting()).toBe(true)
        expect(await cancelButton.isExisting()).toBe(true)
        expect(await okButton.getText()).toBe('OK')
        expect(await cancelButton.getText()).toBe('Cancel')

        // Close dialog with Escape
        await browser.keys('Escape')

        // Wait for dialog to close
        const modalOverlay = await browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 3000, reverse: true })
    })

    it('creates a folder and closes the dialog', async () => {
        await ensureAppReadyWithFocus()

        // Press F7 to open new folder dialog
        await browser.keys('F7')

        // Wait for dialog to appear
        const dialog = await browser.$(MKDIR_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Type a unique folder name
        const folderName = `test-folder-${Date.now()}`
        const nameInput = await browser.$(`${MKDIR_DIALOG} .name-input`)
        await nameInput.waitForExist({ timeout: 3000 })
        await nameInput.setValue(folderName)
        await browser.pause(200)

        // Verify OK button is enabled
        const okButton = (await browser.$(`${MKDIR_DIALOG} button.primary`)) as unknown as WebdriverIO.Element
        expect(await okButton.isEnabled()).toBe(true)

        // Click OK to create the folder
        await jsClick(okButton)

        // Wait for dialog to close (confirms create_directory succeeded)
        const modalOverlay = await browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 5000, reverse: true })
    })
})

describe('Copy dialog', () => {
    it('opens copy dialog with F5', async () => {
        await ensureAppReadyWithFocus()

        // Move cursor to a file (skip ".." entry)
        const cursorEntry = (await browser.$('.file-entry.is-under-cursor')) as unknown as WebdriverIO.Element
        const cursorText = await getEntryName(cursorEntry)

        if (cursorText === '..') {
            await browser.keys('ArrowDown')
            await browser.pause(300)
        }

        // Press F5 to open copy dialog
        await browser.keys('F5')

        // Wait for copy dialog to appear
        const modalOverlay = await browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 5000 })

        const copyDialog = await browser.$(COPY_DIALOG)
        await copyDialog.waitForExist({ timeout: 5000 })

        // Verify dialog has path input
        const pathInput = await browser.$(`${COPY_DIALOG} .path-input`)
        expect(await pathInput.isExisting()).toBe(true)

        // Verify dialog has Copy and Cancel buttons
        const copyButton = await browser.$(`${COPY_DIALOG} button.primary`)
        const cancelButton = await browser.$(`${COPY_DIALOG} button.secondary`)
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
