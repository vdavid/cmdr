/**
 * E2E tests for the file viewer on Linux.
 *
 * Uses SvelteKit's link-click interception for client-side navigation.
 * browser.url() doesn't work in Tauri (navigates to about:blank with DNS error),
 * and pushState+popstate doesn't trigger SvelteKit's router. Creating a temporary
 * <a> element and clicking it does trigger SvelteKit's client-side navigation.
 *
 * Test file: Uses a text file from the shared E2E fixtures (left/file-a.txt).
 * Previously used /root/test-dir/test-file.txt created by Docker's entrypoint.sh,
 * which only worked in Docker — not on native Linux VMs.
 */

import path from 'path'

// Use fixture file from the shared E2E fixture tree (created by wdio.conf.ts onPrepare)
const fixtureRoot = process.env.CMDR_E2E_START_PATH ?? '/tmp/cmdr-e2e-fallback'
const testFilePath = path.join(fixtureRoot, 'left', 'file-a.txt')

/** Navigate to a SvelteKit route via link-click interception. */
async function navigateToViewerRoute(path: string): Promise<void> {
    await browser.execute((p: string) => {
        const a = document.createElement('a')
        a.href = p
        document.body.appendChild(a)
        a.click()
        a.remove()
    }, path)
}

/**
 * Ensure the main app is loaded, then navigate to the viewer.
 * If we're not on the main route (previous describe left us elsewhere),
 * navigate back to "/" first so SvelteKit has a valid starting point.
 */
async function navigateToViewer(filePath?: string): Promise<void> {
    const hasExplorer = await browser.$('.dual-pane-explorer').isExisting()
    if (!hasExplorer) {
        await navigateToViewerRoute('/')
        const explorer = browser.$('.dual-pane-explorer')
        await explorer.waitForExist({ timeout: 15000 })
    }

    const viewerPath = filePath ? `/viewer?path=${encodeURIComponent(filePath)}` : '/viewer'
    await navigateToViewerRoute(viewerPath)
}

/**
 * Navigates to the viewer for a given file and waits for file content to render.
 * Throws a diagnostic error if the viewer loads but file content doesn't appear.
 */
async function navigateAndWaitForViewer(filePath: string): Promise<void> {
    await navigateToViewer(filePath)

    const viewer = browser.$('.viewer-container')
    await viewer.waitForExist({ timeout: 15000 })

    const fileContent = browser.$('.file-content')
    try {
        await fileContent.waitForExist({ timeout: 10000 })
    } catch {
        const statusMsg = browser.$('.status-message')
        if (await statusMsg.isExisting()) {
            const text = await statusMsg.getText()
            throw new Error(`Viewer did not load file content. Status: "${text}"`)
        }
        throw new Error('Viewer did not load file content and no status message found')
    }
}

describe('File viewer', () => {
    before(async () => {
        await navigateAndWaitForViewer(testFilePath)
    })

    it('renders the viewer container', async () => {
        const viewer = browser.$('.viewer-container')
        expect(await viewer.isExisting()).toBe(true)
    })

    it('displays file content with line elements', async () => {
        const fileContent = browser.$('.file-content')
        expect(await fileContent.isExisting()).toBe(true)

        const lines = await browser.$$('.line')
        expect(lines.length).toBeGreaterThan(0)
    })

    it('shows file name in status bar', async () => {
        const statusBar = browser.$('.status-bar')
        const statusText = await statusBar.getText()
        expect(statusText).toContain('file-a.txt')
    })

    it('shows line count in status bar', async () => {
        const statusBar = browser.$('.status-bar')
        const statusText = await statusBar.getText()
        // file-a.txt contains 1024 bytes of 'A' (no newlines) = 1 line
        expect(statusText).toContain('1 line')
    })

    it('shows file size in status bar', async () => {
        const statusBar = browser.$('.status-bar')
        const statusText = await statusBar.getText()
        // file-a.txt is 1024 bytes = 1 KB
        expect(statusText).toContain('KB')
    })

    it('shows backend mode badge', async () => {
        const badge = browser.$('.backend-badge')
        expect(await badge.isExisting()).toBe(true)

        // Small file should be loaded entirely in memory
        const badgeText = await badge.getText()
        expect(badgeText).toBe('in memory')
    })
})

describe('File viewer search', () => {
    before(async () => {
        await navigateAndWaitForViewer(testFilePath)
    })

    it('opens search bar with Ctrl+F', async () => {
        await browser.keys(['Control', 'f'])

        const searchBar = browser.$('.search-bar')
        await searchBar.waitForExist({ timeout: 5000 })
        expect(await searchBar.isExisting()).toBe(true)
    })

    it('finds matches in file content', async () => {
        const searchInput = browser.$('.search-input')
        await searchInput.waitForExist({ timeout: 5000 })

        await searchInput.setValue('AAA')

        // Wait for search results to appear (debounced search + backend poll)
        const matchCount = browser.$('.match-count')
        await browser.waitUntil(
            async () => {
                if (!(await matchCount.isExisting())) return false
                const text = await matchCount.getText()
                return text.includes('of')
            },
            { timeout: 5000 },
        )
        const matchText = await matchCount.getText()
        // file-a.txt is all 'A' characters, so there should be at least one match
        expect(matchText).toContain('of')
    })

    it('closes search with Escape', async () => {
        let searchBar = browser.$('.search-bar')
        expect(await searchBar.isExisting()).toBe(true)

        await browser.keys('Escape')

        // Wait for search bar to close
        await browser.waitUntil(async () => !(await browser.$('.search-bar').isExisting()), { timeout: 3000 })

        searchBar = browser.$('.search-bar')
        expect(await searchBar.isExisting()).toBe(false)
    })
})

describe('File viewer error handling', () => {
    it('shows error for missing file path', async () => {
        await navigateToViewer()

        const viewer = browser.$('.viewer-container')
        await viewer.waitForExist({ timeout: 15000 })

        const statusMsg = browser.$('.status-message')
        await statusMsg.waitForExist({ timeout: 10000 })

        const text = await statusMsg.getText()
        expect(text).toContain('No file path')
    })
})
