/**
 * E2E tests for the Cmdr Tauri application on macOS.
 *
 * These tests run against the actual Tauri app using CrabNebula's WebDriver
 * for macOS (WKWebView). They verify real application behavior including
 * frontend-backend integration.
 *
 * CrabNebula WebDriver quirks addressed in these tests:
 * - browser.keys() doesn't deliver key events — use dispatchKey() via JS
 * - Element references in browser.execute() args aren't serialized — use
 *   querySelector inside execute() instead of passing element refs
 *
 * Usage:
 *   export CN_API_KEY=<your-key>
 *   pnpm test:e2e:macos
 */

import { ensureAppReady, MKDIR_DIALOG, TRANSFER_DIALOG } from '../e2e-shared/helpers.js'

// ─── Helpers ────────────────────────────────────────────────────────────────

/**
 * Dispatches a keyboard event via JavaScript. CrabNebula's WebDriver doesn't
 * deliver browser.keys() to the app, so we dispatch events directly on the
 * focused element or the explorer container.
 */
async function dispatchKey(key: string): Promise<void> {
    await browser.execute((k: string) => {
        const target = document.querySelector('.dual-pane-explorer') ?? document.activeElement ?? document.body
        target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
        target.dispatchEvent(new KeyboardEvent('keyup', { key: k, bubbles: true, cancelable: true }))
    }, key)
    await browser.pause(300)
}

/** Returns the cursor index in the focused pane via JS (avoids element ref issues). */
async function getCursorIndex(): Promise<number> {
    return browser.execute(() => {
        const pane = document.querySelector('.file-pane.is-focused') ?? document.querySelector('.file-pane')
        if (!pane) return -1
        const entries = pane.querySelectorAll('.file-entry')
        for (let i = 0; i < entries.length; i++) {
            if (entries[i].classList.contains('is-under-cursor')) return i
        }
        return -1
    })
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

        const entryCount = await browser.execute(() => {
            const pane = document.querySelector('.file-pane.is-focused') ?? document.querySelector('.file-pane')
            return pane?.querySelectorAll('.file-entry').length ?? 0
        })
        if (entryCount < 2) {
            console.log('Skipping arrow key test: fewer than 2 entries')
            return
        }

        const initialIndex = await getCursorIndex()
        expect(initialIndex).toBeGreaterThanOrEqual(0)

        await dispatchKey('ArrowDown')

        const newIndex = await getCursorIndex()
        expect(newIndex).toBeGreaterThanOrEqual(0)
        expect(newIndex).not.toBe(initialIndex)
    })

    it('switches panes with Tab key', async () => {
        await ensureAppReady()

        // Verify left pane is focused after ensureAppReady
        const initialFocus = await browser.execute(() => {
            const panes = document.querySelectorAll('.file-pane')
            return {
                leftFocused: panes[0]?.classList.contains('is-focused') ?? false,
                rightFocused: panes[1]?.classList.contains('is-focused') ?? false,
            }
        })
        expect(initialFocus.leftFocused).toBe(true)

        // Dispatch Tab to switch to right pane
        await dispatchKey('Tab')

        const afterTab = await browser.execute(() => {
            const panes = document.querySelectorAll('.file-pane')
            return {
                leftFocused: panes[0]?.classList.contains('is-focused') ?? false,
                rightFocused: panes[1]?.classList.contains('is-focused') ?? false,
            }
        })
        expect(afterTab.rightFocused).toBe(true)
        expect(afterTab.leftFocused).toBe(false)

        // Tab again to go back to left pane
        await dispatchKey('Tab')

        const afterSecondTab = await browser.execute(() => {
            const panes = document.querySelectorAll('.file-pane')
            return panes[0]?.classList.contains('is-focused') ?? false
        })
        expect(afterSecondTab).toBe(true)
    })

    it('toggles selection with Space key', async () => {
        await ensureAppReady()

        // Skip ".." entry
        const cursorText = await browser.execute(() => {
            const entry = document.querySelector('.file-entry.is-under-cursor')
            return entry?.querySelector('.col-name')?.textContent ?? entry?.querySelector('.name')?.textContent ?? ''
        })
        if (cursorText === '..') {
            await dispatchKey('ArrowDown')
        }

        // Verify not selected initially
        const initialSelected = await browser.execute(() => {
            return document.querySelector('.file-entry.is-under-cursor')?.classList.contains('is-selected') ?? false
        })
        expect(initialSelected).toBe(false)

        // Space to select
        await dispatchKey(' ')

        const afterSelect = await browser.execute(() => {
            return document.querySelector('.file-entry.is-under-cursor')?.classList.contains('is-selected') ?? false
        })
        expect(afterSelect).toBe(true)

        // Space again to deselect
        await dispatchKey(' ')

        const afterDeselect = await browser.execute(() => {
            return document.querySelector('.file-entry.is-under-cursor')?.classList.contains('is-selected') ?? false
        })
        expect(afterDeselect).toBe(false)
    })
})

describe('Mouse interactions', () => {
    it('moves cursor when clicking a file entry', async () => {
        await ensureAppReady()

        const entryCount = await browser.execute(() => {
            const pane = document.querySelector('.file-pane.is-focused') ?? document.querySelector('.file-pane')
            return pane?.querySelectorAll('.file-entry').length ?? 0
        })
        if (entryCount < 2) return

        // Click the second entry via querySelector (element refs don't work in CrabNebula)
        await browser.execute(() => {
            const pane = document.querySelector('.file-pane.is-focused') ?? document.querySelector('.file-pane')
            const entries = pane?.querySelectorAll('.file-entry')
            ;(entries?.[1] as HTMLElement | undefined)?.click()
        })
        await browser.pause(300)

        const cursorIndex = await getCursorIndex()
        expect(cursorIndex).toBe(1)
    })

    it('switches pane focus when clicking other pane', async () => {
        await ensureAppReady()

        // Click a file entry in the right pane
        await browser.execute(() => {
            const rightPane = document.querySelectorAll('.file-pane')[1]
            const entry = rightPane?.querySelector('.file-entry')
            entry?.click()
        })
        await browser.pause(300)

        const rightFocused = await browser.execute(() => {
            return document.querySelectorAll('.file-pane')[1]?.classList.contains('is-focused') ?? false
        })
        expect(rightFocused).toBe(true)

        // Click a file entry in the left pane to transfer focus back
        await browser.execute(() => {
            const leftPane = document.querySelectorAll('.file-pane')[0]
            const entry = leftPane?.querySelector('.file-entry')
            entry?.click()
        })
        await browser.pause(300)

        const leftFocused = await browser.execute(() => {
            return document.querySelectorAll('.file-pane')[0]?.classList.contains('is-focused') ?? false
        })
        expect(leftFocused).toBe(true)
    })
})

describe('New folder dialog', () => {
    it('opens new folder dialog with F7', async () => {
        await ensureAppReady()

        await dispatchKey('F7')

        const dialog = browser.$(MKDIR_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        const title = browser.$(`${MKDIR_DIALOG} h2`)
        expect(await title.getText()).toBe('New folder')

        const nameInput = browser.$(`${MKDIR_DIALOG} .name-input`)
        expect(await nameInput.isExisting()).toBe(true)

        // Close dialog via JS dispatch (browser.keys doesn't work)
        await browser.execute(() => {
            document.activeElement?.dispatchEvent(
                new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
            )
        })
        await browser.pause(500)

        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 3000, reverse: true })
    })
})

describe('Transfer dialogs', () => {
    it('opens copy dialog with F5', async () => {
        await ensureAppReady()

        // Skip ".." entry
        const cursorText = await browser.execute(() => {
            const entry = document.querySelector('.file-entry.is-under-cursor')
            return entry?.querySelector('.col-name')?.textContent ?? entry?.querySelector('.name')?.textContent ?? ''
        })
        if (cursorText === '..') {
            await dispatchKey('ArrowDown')
        }

        await dispatchKey('F5')

        const dialog = browser.$(TRANSFER_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        const title = browser.$(`${TRANSFER_DIALOG} h2`)
        expect(await title.getText()).toContain('Copy')

        // Close dialog
        await browser.execute(() => {
            document.activeElement?.dispatchEvent(
                new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
            )
        })
        await browser.pause(500)

        const modalAfter = browser.$('.modal-overlay')
        expect(await modalAfter.isExisting()).toBe(false)
    })
})
