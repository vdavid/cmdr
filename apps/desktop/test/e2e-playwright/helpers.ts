/**
 * Shared Playwright helpers for Cmdr E2E tests.
 *
 * These replace the WebDriverIO-based helpers from e2e-shared/helpers.ts,
 * using the TauriPage API instead of the `browser` global.
 *
 * Key differences from WebDriverIO:
 * - No jsClick() workaround needed — tauriPage.click() works on all elements
 * - No pressSpaceKey() workaround — keyboard.press('Space') works directly
 * - No Backspace dispatchEvent hack — keyboard.press('Backspace') works
 * - evaluate() takes a string expression, not a function
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

/** Union type for tauriPage — works in both Tauri and browser mode. */
type PageLike = TauriPage | BrowserPageAdapter

// ── Selectors ────────────────────────────────────────────────────────────────

export const MKDIR_DIALOG = '[data-dialog-id="mkdir-confirmation"]'
export const TRANSFER_DIALOG = '[data-dialog-id="transfer-confirmation"]'

// ── Platform helpers ─────────────────────────────────────────────────────────

export const CTRL_OR_META = process.platform === 'darwin' ? 'Meta' : 'Control'

// ── App readiness ────────────────────────────────────────────────────────────

/**
 * Ensures the app is fully loaded and focus is initialized.
 * Waits for file entries, dismisses overlays, clicks a file entry in the left
 * pane, and focuses the explorer container so keyboard events reach the handler.
 */
export async function ensureAppReady(tauriPage: PageLike): Promise<void> {
    // Wait for file entries to be visible (confirms app is fully loaded)
    await tauriPage.waitForSelector('.file-entry', 15000)

    // Wait for the HTML loading screen to be gone
    await tauriPage.waitForFunction(
        '!document.querySelector("#loading-screen") || document.querySelector("#loading-screen").style.display === "none" || !document.querySelector("#loading-screen").offsetParent',
        5000,
    )

    // Close any lingering modal dialog from a prior test
    await tauriPage.evaluate(`(function() {
        var overlay = document.querySelector('.modal-overlay');
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    })()`)
    // Wait until the modal overlay is gone
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)

    // Dismiss any overlays (AI notification, etc.)
    await tauriPage.evaluate(`(function() {
        var btn = document.querySelector('.ai-notification .ai-button.secondary');
        if (btn) btn.click();
    })()`)
    // Wait until the AI notification is gone
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.ai-notification')), 3000)

    // Click on a file entry in the left pane to ensure focus, then focus the
    // explorer container so keyboard events reach the handler.
    await tauriPage.evaluate(`(function() {
        var entry = document.querySelector('.file-pane .file-entry');
        if (entry) entry.click();
        var explorer = document.querySelector('.dual-pane-explorer');
        if (explorer) explorer.focus();
    })()`)

    // Wait until a file entry has the cursor (focus confirmed)
    await tauriPage.waitForSelector('.file-pane .file-entry.is-under-cursor', 3000)
}

// ── DOM query helpers ────────────────────────────────────────────────────────

/** Gets file entry name text from the cursor entry. Works with both view modes. */
export async function getEntryName(tauriPage: PageLike, selector: string): Promise<string> {
    return tauriPage.evaluate<string>(`(function() {
        var entry = document.querySelector('${selector}');
        if (!entry) return '';
        var colName = entry.querySelector('.col-name');
        if (colName) return colName.textContent || '';
        var name = entry.querySelector('.name');
        if (name) return name.textContent || '';
        return entry.textContent || '';
    })()`)
}

/** Checks whether a given filename exists in the focused pane's DOM listing. */
export async function fileExistsInFocusedPane(tauriPage: PageLike, targetName: string): Promise<boolean> {
    return tauriPage.evaluate<boolean>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return false;
        var entries = pane.querySelectorAll('.file-entry');
        return Array.from(entries).some(function(e) {
            return (e.querySelector('.col-name') || e.querySelector('.name') || {}).textContent === ${JSON.stringify(targetName)};
        });
    })()`)
}

/** Checks whether a given filename exists in a specific pane (left=0, right=1). */
export async function fileExistsInPane(tauriPage: PageLike, targetName: string, paneIndex: number): Promise<boolean> {
    return tauriPage.evaluate<boolean>(`(function() {
        var panes = document.querySelectorAll('.file-pane');
        var pane = panes[${paneIndex}];
        if (!pane) return false;
        var entries = pane.querySelectorAll('.file-entry');
        return Array.from(entries).some(function(e) {
            return (e.querySelector('.col-name') || e.querySelector('.name') || {}).textContent === ${JSON.stringify(targetName)};
        });
    })()`)
}

/**
 * Finds the index of a file by name in the focused pane's entry list.
 * Returns the target index and total entry count, or an error object.
 */
export async function findFileIndex(
    tauriPage: PageLike,
    fileName: string,
): Promise<{ targetIndex: number; total: number } | { error: string }> {
    return tauriPage.evaluate<{ targetIndex: number; total: number } | { error: string }>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return { error: 'no focused pane' };
        var entries = pane.querySelectorAll('.file-entry');
        var targetIndex = -1;
        for (var i = 0; i < entries.length; i++) {
            var text = (entries[i].querySelector('.col-name') || entries[i].querySelector('.name') || {}).textContent || '';
            if (text === ${JSON.stringify(fileName)}) {
                targetIndex = i;
                break;
            }
        }
        return { targetIndex: targetIndex, total: entries.length };
    })()`)
}

// ── Cursor helpers ──────────────────────────────────────────────────────────

/** If the cursor is on the ".." parent entry, moves it down one position. */
export async function skipParentEntry(tauriPage: PageLike): Promise<void> {
    const cursorText = await tauriPage.evaluate<string>(`(function() {
        var entry = document.querySelector('.file-entry.is-under-cursor');
        if (!entry) return '';
        return (entry.querySelector('.col-name') || entry.querySelector('.name') || {}).textContent || '';
    })()`)
    if (cursorText === '..') {
        await tauriPage.keyboard.press('ArrowDown')
        await pollUntil(
            tauriPage,
            async () => {
                const name = await tauriPage.evaluate<string>(`(function() {
                    var entry = document.querySelector('.file-entry.is-under-cursor');
                    if (!entry) return '';
                    return (entry.querySelector('.col-name') || entry.querySelector('.name') || {}).textContent || '';
                })()`)
                return name !== '..'
            },
            3000,
        )
    }
}

/**
 * Moves the cursor to a specific file by name in the focused pane.
 * Uses findFileIndex() for DOM reading, then navigates with keyboard.
 */
export async function moveCursorToFile(tauriPage: PageLike, targetName: string): Promise<boolean> {
    const info = await findFileIndex(tauriPage, targetName)
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

// ── Navigation helpers ──────────────────────────────────────────────────────

/**
 * Navigate to a SvelteKit route via link-click interception.
 * browser.url() doesn't work in Tauri, so we create a temporary `<a>` element
 * and click it to trigger SvelteKit's client-side routing.
 */
export async function navigateToRoute(tauriPage: PageLike, path: string): Promise<void> {
    await tauriPage.evaluate(`(function() {
        var a = document.createElement('a');
        a.href = ${JSON.stringify(path)};
        document.body.appendChild(a);
        a.click();
        a.remove();
    })()`)
}

// ── Fixture helpers ─────────────────────────────────────────────────────────

/** Returns the fixture root path from the CMDR_E2E_START_PATH environment variable. */
export function getFixtureRoot(): string {
    const root = process.env.CMDR_E2E_START_PATH
    if (!root) throw new Error('CMDR_E2E_START_PATH env var is not set')
    return root
}

// ── Command palette ─────────────────────────────────────────────────────────

/**
 * Executes a command via the command palette. Opens the palette, types the
 * query, and clicks the first matching result.
 */
export async function executeViaCommandPalette(tauriPage: PageLike, query: string): Promise<void> {
    await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${CTRL_OR_META === 'Control'}, metaKey: ${CTRL_OR_META === 'Meta'}, shiftKey: true, bubbles: true
    }))`)
    await tauriPage.waitForSelector('.palette-overlay', 5000)
    await tauriPage.fill('.palette-overlay .search-input', query)
    // Wait for filtered results to appear
    await tauriPage.waitForSelector('.palette-overlay .result-item', 3000)
    await tauriPage.evaluate(`(function() {
        var item = document.querySelector('.palette-overlay .result-item');
        if (item) item.click();
    })()`)
    // Wait for palette to close after executing the command
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.palette-overlay')), 3000)
}

// ── Utility ─────────────────────────────────────────────────────────────────

export function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms))
}

/**
 * Polls a condition function until it returns true or timeout is reached.
 * Similar to WebDriverIO's browser.waitUntil().
 */
export async function pollUntil(
    _page: PageLike,
    condition: () => Promise<boolean>,
    timeout: number,
    interval = 100,
): Promise<boolean> {
    const deadline = Date.now() + timeout
    while (Date.now() < deadline) {
        try {
            if (await condition()) return true
        } catch {
            // Element might not exist yet — keep polling
        }
        await sleep(interval)
    }
    return false
}
