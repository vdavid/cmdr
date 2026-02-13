/**
 * E2E tests for the file viewer on Linux.
 *
 * Uses SvelteKit's link-click interception for client-side navigation.
 * browser.url() doesn't work in Tauri (navigates to about:blank with DNS error),
 * and pushState+popstate doesn't trigger SvelteKit's router. Creating a temporary
 * <a> element and clicking it does trigger SvelteKit's client-side navigation.
 *
 * Test file: /root/test-dir/test-file.txt (created by entrypoint.sh,
 * contains "test content" — a single line).
 */

const testFilePath = '/root/test-dir/test-file.txt'

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

describe('File viewer', () => {
    before(async () => {
        // Wait for the main app to fully load on first launch
        const explorer = browser.$('.dual-pane-explorer')
        await explorer.waitForExist({ timeout: 15000 })

        // Navigate to viewer via SvelteKit client-side routing
        await navigateToViewerRoute(`/viewer?path=${encodeURIComponent(testFilePath)}`)

        // Wait for viewer to load
        const viewer = browser.$('.viewer-container')
        await viewer.waitForExist({ timeout: 15000 })

        // Wait for file content to render (viewerOpen() Tauri IPC must succeed)
        const fileContent = browser.$('.file-content')
        try {
            await fileContent.waitForExist({ timeout: 10000 })
        } catch {
            // Diagnostic: check what state the viewer is in
            const statusMsg = browser.$('.status-message')
            if (await statusMsg.isExisting()) {
                const text = await statusMsg.getText()
                throw new Error(`Viewer did not load file content. Status: "${text}"`)
            }
            throw new Error('Viewer did not load file content and no status message found')
        }
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
        expect(statusText).toContain('test-file.txt')
    })

    it('shows line count in status bar', async () => {
        const statusBar = browser.$('.status-bar')
        const statusText = await statusBar.getText()
        // "test content\n" (from echo) = 2 lines
        expect(statusText).toContain('2 lines')
    })

    it('shows file size in status bar', async () => {
        const statusBar = browser.$('.status-bar')
        const statusText = await statusBar.getText()
        // "test content\n" is 13 bytes
        expect(statusText).toContain('B')
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
        await navigateToViewer(testFilePath)

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
    })

    it('opens search bar with Ctrl+F', async () => {
        await browser.keys(['Control', 'f'])
        await browser.pause(300)

        const searchBar = browser.$('.search-bar')
        await searchBar.waitForExist({ timeout: 5000 })
        expect(await searchBar.isExisting()).toBe(true)
    })

    it('finds matches in file content', async () => {
        const searchInput = browser.$('.search-input')
        await searchInput.waitForExist({ timeout: 5000 })

        await searchInput.setValue('test')
        // Wait for debounced search + poll to return results
        await browser.pause(1000)

        const matchCount = browser.$('.match-count')
        const matchText = await matchCount.getText()
        // Should show "1 of 1" or similar match count
        expect(matchText).toContain('1')
    })

    it('closes search with Escape', async () => {
        let searchBar = browser.$('.search-bar')
        expect(await searchBar.isExisting()).toBe(true)

        await browser.keys('Escape')
        await browser.pause(300)

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
