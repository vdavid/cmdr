/**
 * E2E tests for file operations in the Cmdr Tauri application on macOS.
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
 * CrabNebula WebDriver quirks addressed in these tests:
 * - browser.keys() doesn't deliver key events — use dispatchKey() via JS
 * - Element references in browser.execute() args aren't serialized — use
 *   querySelector inside execute() instead of passing element refs
 */

import fs from 'fs'
import path from 'path'
import { ensureAppReady, fileExistsInFocusedPane, fileExistsInPane, findFileIndex, TRANSFER_DIALOG } from '../e2e-shared/helpers.js'

// ── Helpers ──────────────────────────────────────────────────────────────────

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

/**
 * Moves the cursor to a specific file by name in the focused pane.
 * Uses findFileIndex() for DOM reading, then navigates with dispatchKey()
 * to preserve focus.
 */
async function moveCursorToFile(fileName: string): Promise<boolean> {
    const info = await findFileIndex(fileName)

    if ('error' in info || info.targetIndex < 0) return false

    await dispatchKey('Home')
    for (let i = 0; i < info.targetIndex; i++) {
        await dispatchKey('ArrowDown')
    }
    return true
}

/** Returns the fixture root path from the environment variable. */
function getFixtureRoot(): string {
    const root = process.env.CMDR_E2E_START_PATH
    if (!root) throw new Error('CMDR_E2E_START_PATH env var is not set')
    return root
}

/**
 * Confirms the transfer dialog by clicking the primary button.
 * Uses querySelector inside execute (element refs don't serialize in CrabNebula).
 */
async function confirmTransferDialog(): Promise<void> {
    await browser.execute(() => {
        const btn = document.querySelector('[data-dialog-id="transfer-confirmation"] button.primary') as HTMLElement | null
        btn?.click()
    })
    await browser.pause(300)
}

/** Closes any open modal dialog by dispatching Escape via JS. */
async function dismissDialog(): Promise<void> {
    await browser.execute(() => {
        document.activeElement?.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
        )
    })
    await browser.pause(500)
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('Copy on APFS', () => {
    it('copies file-a.txt from left pane to right pane via F5', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        // Move cursor to file-a.txt
        const found = await moveCursorToFile('file-a.txt')
        expect(found).toBe(true)

        // Press F5 to open copy dialog
        await dispatchKey('F5')

        // Wait for transfer dialog to appear
        const dialog = browser.$(TRANSFER_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Verify title contains "Copy"
        const title = browser.$(`${TRANSFER_DIALOG} h2`)
        expect(await title.getText()).toContain('Copy')

        // Click the Copy button to confirm
        await confirmTransferDialog()

        // Wait for dialog to close (confirms copy succeeded)
        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 10000, reverse: true })

        // Switch to right pane to verify the file appeared in DOM
        await dispatchKey('Tab')

        await browser.waitUntil(
            async () => fileExistsInFocusedPane('file-a.txt'),
            { timeout: 5000, timeoutMsg: 'file-a.txt did not appear in right pane after copy' },
        )

        // Verify on disk: file exists in right dir
        expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt'))).toBe(true)

        // Verify original still exists in left dir (copy, not move)
        expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
    })
})

describe('Move on APFS', () => {
    it('moves file-b.txt from left pane to right pane via F6', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        // Move cursor to file-b.txt
        const found = await moveCursorToFile('file-b.txt')
        expect(found).toBe(true)

        // Press F6 to open move dialog
        await dispatchKey('F6')

        // Wait for transfer dialog to appear
        const dialog = browser.$(TRANSFER_DIALOG)
        await dialog.waitForExist({ timeout: 5000 })

        // Verify title contains "Move"
        const title = browser.$(`${TRANSFER_DIALOG} h2`)
        expect(await title.getText()).toContain('Move')

        // Click the Move button to confirm
        await confirmTransferDialog()

        // Wait for dialog to close (confirms move succeeded)
        const modalOverlay = browser.$('.modal-overlay')
        await modalOverlay.waitForExist({ timeout: 10000, reverse: true })

        // Verify file-b.txt is gone from left pane DOM
        await browser.waitUntil(
            async () => !(await fileExistsInPane('file-b.txt', 0)),
            { timeout: 5000, timeoutMsg: 'file-b.txt did not disappear from left pane after move' },
        )

        // Switch to right pane and verify file-b.txt appeared
        await dispatchKey('Tab')

        await browser.waitUntil(
            async () => fileExistsInFocusedPane('file-b.txt'),
            { timeout: 5000, timeoutMsg: 'file-b.txt did not appear in right pane after move' },
        )

        // Verify on disk: file is gone from left, present in right
        expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-b.txt'))).toBe(false)
        expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-b.txt'))).toBe(true)
    })
})

// Delete (F8) is not yet implemented — the F8 button in FunctionKeyBar.svelte is
// disabled with the aria-label "Delete (F8) — not yet available". No keyboard
// handler exists for F8 in DualPaneExplorer. Skipping this test until F8 is wired up.

describe('Volume list renders', () => {
    it('shows at least one macOS volume in the volume breadcrumb dropdown', async () => {
        await ensureAppReady()

        // Click the volume breadcrumb label in the left pane to open the dropdown.
        // The volume breadcrumb is inside .file-pane, rendered as .volume-name.
        await browser.execute(() => {
            const leftPane = document.querySelectorAll('.file-pane')[0]
            const volumeName = leftPane?.querySelector('.volume-name') as HTMLElement | null
            volumeName?.click()
        })
        await browser.pause(500)

        // Wait for the volume dropdown to appear
        const dropdown = browser.$('.volume-dropdown')
        await dropdown.waitForExist({ timeout: 5000 })

        // Verify at least one volume-item exists in the dropdown
        const volumeCount = await browser.execute(() => {
            const items = document.querySelectorAll('.volume-dropdown .volume-item')
            return items.length
        })
        expect(volumeCount).toBeGreaterThanOrEqual(1)

        // Close the dropdown by pressing Escape
        await dismissDialog()
    })
})

describe('Navigate into directory with Enter', () => {
    it('enters sub-dir and shows nested-file.txt', async () => {
        await ensureAppReady()

        // Move cursor to sub-dir
        const found = await moveCursorToFile('sub-dir')
        expect(found).toBe(true)

        // Press Enter to navigate into the directory
        await dispatchKey('Enter')

        // Wait for nested-file.txt to appear in the listing (confirms navigation)
        await browser.waitUntil(
            async () => fileExistsInFocusedPane('nested-file.txt'),
            { timeout: 5000, timeoutMsg: 'nested-file.txt did not appear after entering sub-dir' },
        )
    })
})

describe('Navigate to parent with Backspace', () => {
    it('goes back to left/ from sub-dir via Backspace', async () => {
        await ensureAppReady()

        // First, navigate into sub-dir
        const found = await moveCursorToFile('sub-dir')
        expect(found).toBe(true)

        await dispatchKey('Enter')

        // Wait until we're inside sub-dir (nested-file.txt visible)
        await browser.waitUntil(
            async () => fileExistsInFocusedPane('nested-file.txt'),
            { timeout: 5000, timeoutMsg: 'nested-file.txt did not appear after entering sub-dir' },
        )

        // Press Backspace to go to parent
        await dispatchKey('Backspace')

        // Wait for file-a.txt to appear (confirms we're back in left/)
        await browser.waitUntil(
            async () => fileExistsInFocusedPane('file-a.txt'),
            { timeout: 5000, timeoutMsg: 'file-a.txt did not appear after navigating to parent' },
        )

        // Also confirm sub-dir is visible (we're in the parent that contains it)
        const hasSubDir = await fileExistsInFocusedPane('sub-dir')
        expect(hasSubDir).toBe(true)
    })
})
