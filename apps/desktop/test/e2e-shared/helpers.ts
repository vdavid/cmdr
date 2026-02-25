/**
 * Shared E2E helpers for DOM queries and selectors used by both Linux and
 * macOS test suites. These run in the WebDriverIO Node.js process and use
 * the global `browser` object.
 *
 * Platform-specific input helpers (dispatchKey, jsClick, pressSpaceKey)
 * stay in each platform's spec files.
 */

// ── App readiness ────────────────────────────────────────────────────────────

/**
 * Ensures the app is fully loaded and focus is initialized.
 * Waits for file entries, dismisses overlays, clicks a file entry in the left
 * pane, and focuses the explorer container so keyboard events reach the handler.
 *
 * Used by both Linux (WebKitGTK) and macOS (CrabNebula) test suites.
 */
export async function ensureAppReady(): Promise<void> {
    // Wait for file entries to be visible (confirms app is fully loaded)
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

    // Close any lingering modal dialog from a prior test (prevents cascading failures)
    await browser.execute(() => {
        const overlay = document.querySelector('.modal-overlay')
        overlay?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    })
    await browser.pause(300)

    // Dismiss any overlays (AI notification, etc.) via JS click to bypass
    // WebDriver strict clickability checks
    await browser.execute(() => {
        const dismissBtn = document.querySelector('.ai-notification .ai-button.secondary')
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        dismissBtn?.click()
    })
    await browser.pause(300)

    // Click on a file entry in the left pane to ensure focus, then
    // focus the explorer container so keyboard events reach the handler.
    await browser.execute(() => {
        const entry = document.querySelector('.file-pane .file-entry')
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        entry?.click()
        const explorer = document.querySelector('.dual-pane-explorer')
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        explorer?.focus()
    })
    await browser.pause(500)
}

// ── Selectors ────────────────────────────────────────────────────────────────

export const MKDIR_DIALOG = '[data-dialog-id="mkdir-confirmation"]'
export const TRANSFER_DIALOG = '[data-dialog-id="transfer-confirmation"]'

// ── DOM query helpers ────────────────────────────────────────────────────────

/** Gets file entry name text. Works with both Full and Brief view modes. */
export async function getEntryName(entry: WebdriverIO.Element): Promise<string> {
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

/** Checks whether a given filename exists in the focused pane's DOM listing. */
export async function fileExistsInFocusedPane(targetName: string): Promise<boolean> {
    return browser.execute((name: string) => {
        const pane = document.querySelector('.file-pane.is-focused')
        if (!pane) return false
        const entries = pane.querySelectorAll('.file-entry')
        return Array.from(entries).some(
            (e) => (e.querySelector('.col-name') ?? e.querySelector('.name'))?.textContent === name,
        )
    }, targetName)
}

/** Checks whether a given filename exists in a specific pane (left=0, right=1). */
export async function fileExistsInPane(targetName: string, paneIndex: number): Promise<boolean> {
    return browser.execute(
        (name: string, idx: number) => {
            const panes = document.querySelectorAll('.file-pane')
            const pane = panes[idx]
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard; TS lacks noUncheckedIndexedAccess here
            if (!pane) return false
            const entries = pane.querySelectorAll('.file-entry')
            return Array.from(entries).some(
                (e) => (e.querySelector('.col-name') ?? e.querySelector('.name'))?.textContent === name,
            )
        },
        targetName,
        paneIndex,
    )
}

/**
 * Finds the index of a file by name in the focused pane's entry list.
 * Returns the target index and total entry count, or an error string.
 * The caller handles platform-specific keyboard navigation to that index.
 */
export async function findFileIndex(
    fileName: string,
): Promise<{ targetIndex: number; total: number } | { error: string }> {
    return browser.execute((name: string) => {
        const pane = document.querySelector('.file-pane.is-focused')
        if (!pane) return { error: 'no focused pane' }
        const entries = pane.querySelectorAll('.file-entry')
        let targetIndex = -1
        for (let i = 0; i < entries.length; i++) {
            const text =
                entries[i].querySelector('.col-name')?.textContent ??
                entries[i].querySelector('.name')?.textContent ??
                ''
            if (text === name) {
                targetIndex = i
                break
            }
        }
        return { targetIndex, total: entries.length }
    }, fileName)
}
