/**
 * E2E tests for the Cmdr Tauri application on macOS.
 *
 * These tests run against the actual Tauri app using CrabNebula's WebDriver
 * for macOS (WKWebView). They verify real application behavior including
 * frontend-backend integration.
 *
 * Usage:
 *   export CN_API_KEY=<your-key>
 *   pnpm test:e2e:macos
 */

// ─── Helpers ────────────────────────────────────────────────────────────────

/** Gets file entry name text. Works with both Full and Brief view modes. */
async function getEntryName(entry: WebdriverIO.Element): Promise<string> {
    const colName = entry.$('.col-name')
    if (await colName.isExisting()) {
        return colName.getText()
    }
    const name = entry.$('.name')
    if (await name.isExisting()) {
        return name.getText()
    }
    return entry.getText()
}

/**
 * Ensures the app is loaded and focus is initialized.
 * Uses JS execution for dismissing overlays to avoid driver-specific click issues.
 */
async function ensureAppReady(): Promise<void> {
    const fileEntry = browser.$('.file-entry')
    await fileEntry.waitForDisplayed({ timeout: 15000 })

    // Wait for the HTML loading screen to be gone
    const loadingScreen = browser.$('#loading-screen')
    if (await loadingScreen.isExisting()) {
        await browser.waitUntil(async () => !(await loadingScreen.isDisplayed()), {
            timeout: 5000,
            timeoutMsg: 'Loading screen did not disappear',
        })
    }

    // Dismiss any overlays (AI notification, etc.) via JS
    await browser.execute(() => {
        const dismissBtn = document.querySelector('.ai-notification .ai-button.secondary')
        dismissBtn?.click()
    })
    await browser.pause(300)

    // Click on a file entry in the left pane to ensure focus
    await browser.execute(() => {
        const entry = document.querySelector('.file-pane .file-entry')
        entry?.click()
        const explorer = document.querySelector('.dual-pane-explorer')
        explorer?.focus()
    })
    await browser.pause(300)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

describe('Basic rendering', () => {
    it('launches and shows the main window', async () => {
        await browser.pause(3000)
        const title = await browser.getTitle()
        expect(title).toContain('Cmdr')
    })

    it('displays the dual pane interface', async () => {
        const explorer = browser.$('.dual-pane-explorer')
        await explorer.waitForExist({ timeout: 10000 })
        const panes = await browser.$$('.file-pane')
        expect(panes.length).toBe(2)
    })

    it('shows file entries in the panes', async () => {
        const fileEntry = browser.$('.file-entry')
        await fileEntry.waitForExist({ timeout: 10000 })
        const fileEntries = await browser.$$('.file-entry')
        expect(fileEntries.length).toBeGreaterThan(0)
    })
})

describe('Keyboard navigation', () => {
    it('moves cursor with arrow keys', async () => {
        await ensureAppReady()

        const entries = [...(await browser.$$('.file-entry'))]
        if (entries.length < 2) {
            console.log('Skipping arrow key test: fewer than 2 entries')
            return
        }

        let initialCursorIndex = -1
        for (let i = 0; i < entries.length; i++) {
            const cls = await entries[i].getAttribute('class')
            if (cls.includes('is-under-cursor')) {
                initialCursorIndex = i
                break
            }
        }

        expect(initialCursorIndex).toBeGreaterThanOrEqual(0)

        await browser.keys('ArrowDown')
        await browser.pause(300)

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
        expect(newCursorIndex).not.toBe(initialCursorIndex)
    })

    it('switches panes with Tab key', async () => {
        await ensureAppReady()

        let panes = await browser.$$('.file-pane')
        expect(panes.length).toBe(2)

        const leftPaneClass = await panes[0].getAttribute('class')
        expect(leftPaneClass).toContain('is-focused')

        await browser.keys('Tab')
        await browser.pause(300)

        panes = await browser.$$('.file-pane')
        const rightPaneClass = await panes[1].getAttribute('class')
        expect(rightPaneClass).toContain('is-focused')

        const leftPaneClassAfter = await panes[0].getAttribute('class')
        expect(leftPaneClassAfter).not.toContain('is-focused')

        await browser.keys('Tab')
        await browser.pause(300)

        panes = await browser.$$('.file-pane')
        const leftPaneClassFinal = await panes[0].getAttribute('class')
        expect(leftPaneClassFinal).toContain('is-focused')
    })

    it('toggles selection with Space key', async () => {
        await ensureAppReady()

        let cursorEntry = browser.$('.file-entry.is-under-cursor') as unknown as WebdriverIO.Element
        const cursorText = await getEntryName(cursorEntry)

        if (cursorText === '..') {
            await browser.keys('ArrowDown')
            await browser.pause(300)
            cursorEntry = browser.$('.file-entry.is-under-cursor') as unknown as WebdriverIO.Element
        }

        let cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).not.toContain('is-selected')

        // Use W3C Actions API for Space (most reliable across WebDriver implementations)
        await browser.action('key').down(' ').pause(50).up(' ').perform()
        await browser.releaseActions()
        await browser.pause(300)

        cursorEntry = browser.$('.file-entry.is-under-cursor') as unknown as WebdriverIO.Element
        cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).toContain('is-selected')

        await browser.action('key').down(' ').pause(50).up(' ').perform()
        await browser.releaseActions()
        await browser.pause(300)

        cursorEntry = browser.$('.file-entry.is-under-cursor') as unknown as WebdriverIO.Element
        cursorClass = await cursorEntry.getAttribute('class')
        expect(cursorClass).not.toContain('is-selected')
    })
})

describe('Mouse interactions', () => {
    it('moves cursor when clicking a file entry', async () => {
        await ensureAppReady()

        const panes = [...(await browser.$$('.file-pane'))]
        const entries = [...(await panes[0].$$('.file-entry'))]
        if (entries.length < 2) return

        // Use JS click for reliability across WebDriver implementations
        await browser.execute(
            (el: HTMLElement) => {
                el.click()
            },
            entries[1] as unknown as HTMLElement,
        )
        await browser.pause(300)

        const updatedEntries = [...(await panes[0].$$('.file-entry'))]
        const entryClass = await updatedEntries[1].getAttribute('class')
        expect(entryClass).toContain('is-under-cursor')
    })

    it('switches pane focus when clicking other pane', async () => {
        await ensureAppReady()

        let panes = [...(await browser.$$('.file-pane'))]
        expect(panes.length).toBe(2)

        // Click a file entry in the right pane
        await browser.execute(() => {
            const rightPane = document.querySelectorAll('.file-pane')[1]
            const entry = rightPane?.querySelector('.file-entry')
            entry?.click()
        })
        await browser.pause(300)

        panes = [...(await browser.$$('.file-pane'))]
        const rightPaneClass = await panes[1].getAttribute('class')
        expect(rightPaneClass).toContain('is-focused')

        // Click a file entry in the left pane to transfer focus back
        await browser.execute(() => {
            const leftPane = document.querySelectorAll('.file-pane')[0]
            const entry = leftPane?.querySelector('.file-entry')
            entry?.click()
        })
        await browser.pause(300)

        panes = [...(await browser.$$('.file-pane'))]
        const leftPaneClass = await panes[0].getAttribute('class')
        expect(leftPaneClass).toContain('is-focused')
    })
})

describe('New folder dialog', () => {
    it('opens new folder dialog with F7', async () => {
        await ensureAppReady()

        await browser.keys('F7')

        const dialog = browser.$('[data-dialog-id="mkdir-confirmation"]')
        await dialog.waitForExist({ timeout: 5000 })

        const title = browser.$('[data-dialog-id="mkdir-confirmation"] h2')
        expect(await title.getText()).toBe('New folder')

        const nameInput = browser.$('[data-dialog-id="mkdir-confirmation"] .name-input')
        expect(await nameInput.isExisting()).toBe(true)

        // Close dialog
        await browser.keys('Escape')
        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 3000, reverse: true })
    })
})

describe('Transfer dialogs', () => {
    it('opens copy dialog with F5', async () => {
        await ensureAppReady()

        const cursorEntry = browser.$('.file-entry.is-under-cursor') as unknown as WebdriverIO.Element
        const cursorText = await getEntryName(cursorEntry)
        if (cursorText === '..') {
            await browser.keys('ArrowDown')
            await browser.pause(300)
        }

        await browser.keys('F5')

        const dialog = browser.$('[data-dialog-id="transfer-confirmation"]')
        await dialog.waitForExist({ timeout: 5000 })

        const title = browser.$('[data-dialog-id="transfer-confirmation"] h2')
        expect(await title.getText()).toContain('Copy')

        await browser.keys('Escape')
        await browser.pause(300)

        const modalAfter = browser.$('.modal-overlay')
        expect(await modalAfter.isExisting()).toBe(false)
    })
})
