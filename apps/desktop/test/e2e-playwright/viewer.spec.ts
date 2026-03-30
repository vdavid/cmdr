/**
 * E2E tests for the file viewer.
 *
 * Uses SvelteKit's link-click interception for client-side navigation.
 * Test file: Uses a text file from the shared E2E fixtures (left/file-a.txt).
 */

import path from 'path'
import { test, expect } from './fixtures.js'
import { navigateToRoute, pollUntil } from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

// Use fixture file from the shared E2E fixture tree
const fixtureRoot = process.env.CMDR_E2E_START_PATH ?? '/tmp/cmdr-e2e-fallback'
const testFilePath = path.join(fixtureRoot, 'left', 'file-a.txt')

/**
 * Ensure the main app is loaded, then navigate to the viewer.
 * If we're not on the main route, navigate back to "/" first.
 */
async function navigateToViewer(tauriPage: PageLike, filePath?: string): Promise<void> {
    const hasExplorer = await tauriPage.isVisible('.dual-pane-explorer')
    if (!hasExplorer) {
        await navigateToRoute(tauriPage, '/')
        await tauriPage.waitForSelector('.dual-pane-explorer', 15000)
    }

    const viewerPath = filePath ? `/viewer?path=${encodeURIComponent(filePath)}` : '/viewer'
    await navigateToRoute(tauriPage, viewerPath)
}

/**
 * Navigates to the viewer for a given file and waits for file content to render.
 */
async function navigateAndWaitForViewer(tauriPage: PageLike, filePath: string): Promise<void> {
    await navigateToViewer(tauriPage, filePath)

    await tauriPage.waitForSelector('.viewer-container', 15000)

    try {
        await tauriPage.waitForSelector('.file-content', 10000)
    } catch {
        const hasStatusMsg = await tauriPage.isVisible('.status-message')
        if (hasStatusMsg) {
            const text = await tauriPage.textContent('.status-message')
            throw new Error(`Viewer did not load file content. Status: "${text}"`)
        }
        throw new Error('Viewer did not load file content and no status message found')
    }
}

test.describe('File viewer', () => {
    test.beforeAll(async ({ tauriPage }) => {
        await navigateAndWaitForViewer(tauriPage, testFilePath)
    })

    test('renders the viewer container', async ({ tauriPage }) => {
        expect(await tauriPage.isVisible('.viewer-container')).toBe(true)
    })

    test('displays file content with line elements', async ({ tauriPage }) => {
        expect(await tauriPage.isVisible('.file-content')).toBe(true)
        const lineCount = await tauriPage.count('.line')
        expect(lineCount).toBeGreaterThan(0)
    })

    test('shows file name in status bar', async ({ tauriPage }) => {
        const statusText = await tauriPage.textContent('.status-bar')
        expect(statusText).toContain('file-a.txt')
    })

    test('shows line count in status bar', async ({ tauriPage }) => {
        const statusText = await tauriPage.textContent('.status-bar')
        // file-a.txt contains 1024 bytes of 'A' (no newlines) = 1 line
        expect(statusText).toContain('1 line')
    })

    test('shows file size in status bar', async ({ tauriPage }) => {
        const statusText = await tauriPage.textContent('.status-bar')
        // file-a.txt is 1024 bytes = 1 KB
        expect(statusText).toContain('KB')
    })

    test('shows backend mode badge', async ({ tauriPage }) => {
        expect(await tauriPage.isVisible('.backend-badge')).toBe(true)
        const badgeText = await tauriPage.textContent('.backend-badge')
        expect(badgeText).toBe('in memory')
    })
})

test.describe('File viewer search', () => {
    test.beforeAll(async ({ tauriPage }) => {
        await navigateAndWaitForViewer(tauriPage, testFilePath)
    })

    test('opens search bar with Ctrl+F', async ({ tauriPage }) => {
        await tauriPage.keyboard.press('Control+f')

        await tauriPage.waitForSelector('.search-bar', 5000)
        expect(await tauriPage.isVisible('.search-bar')).toBe(true)
    })

    test('finds matches in file content', async ({ tauriPage }) => {
        await tauriPage.waitForSelector('.search-input', 5000)
        await tauriPage.fill('.search-input', 'AAA')

        // Wait for search results (debounced search + backend poll)
        await pollUntil(
            tauriPage,
            async () => {
                const visible = await tauriPage.isVisible('.match-count')
                if (!visible) return false
                const text = await tauriPage.textContent('.match-count')
                return text?.includes('of') ?? false
            },
            5000,
        )

        const matchText = await tauriPage.textContent('.match-count')
        expect(matchText).toContain('of')
    })

    test('closes search with Escape', async ({ tauriPage }) => {
        expect(await tauriPage.isVisible('.search-bar')).toBe(true)

        await tauriPage.keyboard.press('Escape')

        await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.search-bar')), 3000)
        expect(await tauriPage.isVisible('.search-bar')).toBe(false)
    })
})

test.describe('File viewer error handling', () => {
    test('shows error for missing file path', async ({ tauriPage }) => {
        await navigateToViewer(tauriPage)

        await tauriPage.waitForSelector('.viewer-container', 15000)
        await tauriPage.waitForSelector('.status-message', 10000)

        const text = await tauriPage.textContent('.status-message')
        expect(text).toContain('No file path')
    })
})
