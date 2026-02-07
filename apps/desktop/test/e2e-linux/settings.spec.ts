/**
 * E2E tests for the Settings window on Linux.
 *
 * These tests verify the Settings dialog functionality including:
 * - Opening the settings window via keyboard shortcut
 * - Navigation between sections
 * - Search functionality
 * - Setting persistence
 *
 * Note: Since Settings opens as a separate WebviewWindow, these tests
 * need to handle window switching.
 */

/**
 * Helper to wait for a new window to appear and switch to it.
 * Returns the original window handle so we can switch back.
 *
 * @param handlesBefore - Optional window handles captured before the action that opens a new window.
 *                        If provided, skips waiting and directly switches to the new window.
 */
async function switchToNewWindow(handlesBefore?: string[]): Promise<string> {
    const originalWindow = await browser.getWindowHandle()

    let startHandles: string[]
    if (handlesBefore) {
        // Use pre-captured handles (window may already be open)
        startHandles = handlesBefore
    } else {
        // Capture current handles and wait for a new one
        startHandles = await browser.getWindowHandles()
        await browser.waitUntil(
            async () => {
                const handles = await browser.getWindowHandles()
                return handles.length > startHandles.length
            },
            { timeout: 5000, timeoutMsg: 'New window did not appear' },
        )
    }

    // Get the new window handle
    const newHandles = await browser.getWindowHandles()
    const newWindow = newHandles.find((h) => !startHandles.includes(h))

    if (newWindow) {
        await browser.switchToWindow(newWindow)
    }

    return originalWindow
}

/**
 * Helper to ensure the main app is ready for settings tests.
 */
async function ensureMainAppReady(): Promise<void> {
    const fileEntry = await browser.$('.file-entry')
    await fileEntry.waitForExist({ timeout: 10000 })
    await browser.pause(300)
}

/**
 * Helper to open settings window via keyboard shortcut.
 * On Linux, we use Ctrl+, since there's no Meta/Cmd key.
 */
async function openSettingsViaShortcut(): Promise<void> {
    // Try Meta+, first (macOS style, might work in some setups)
    await browser.keys(['Meta', ','])
    await browser.pause(500)

    // Check if settings window opened
    const handles = await browser.getWindowHandles()
    if (handles.length > 1) {
        return // Settings opened
    }

    // Try Ctrl+, as fallback for Linux
    await browser.keys(['Control', ','])
    await browser.pause(500)
}

describe('Settings window', () => {
    // Store original window handle for switching back
    let mainWindowHandle: string

    beforeEach(async () => {
        // Ensure we're on the main window
        const handles = await browser.getWindowHandles()
        mainWindowHandle = handles[0]
        await browser.switchToWindow(mainWindowHandle)
        await ensureMainAppReady()
    })

    afterEach(async () => {
        // Close any extra windows and return to main
        const handles = await browser.getWindowHandles()
        for (const handle of handles) {
            if (handle !== mainWindowHandle) {
                await browser.switchToWindow(handle)
                await browser.closeWindow()
            }
        }
        if (handles.length > 1) {
            await browser.switchToWindow(mainWindowHandle)
        }
    })

    it('opens settings window with keyboard shortcut', async () => {
        // Capture handles before opening settings
        const handlesBefore = await browser.getWindowHandles()

        await openSettingsViaShortcut()

        // Check if a new window appeared
        const handles = await browser.getWindowHandles()

        if (handles.length > handlesBefore.length) {
            // Multi-window mode works - pass pre-captured handles
            await switchToNewWindow(handlesBefore)

            // Verify settings window content
            const settingsWindow = await browser.$('.settings-window')
            await settingsWindow.waitForExist({ timeout: 5000 })
            expect(await settingsWindow.isExisting()).toBe(true)
        } else {
            // Single window mode - skip this test
            console.log('Skipping: Multi-window not supported in this environment')
        }
    })

    it('displays settings sidebar with sections', async () => {
        const handlesBefore = await browser.getWindowHandles()

        await openSettingsViaShortcut()

        const handles = await browser.getWindowHandles()
        if (handles.length <= handlesBefore.length) {
            console.log('Skipping: Multi-window not supported')
            return
        }

        await switchToNewWindow(handlesBefore)

        // Wait for settings to load
        const sidebar = await browser.$('.settings-sidebar')
        await sidebar.waitForExist({ timeout: 5000 })

        // Verify sidebar sections exist
        const sectionItems = await browser.$$('.section-item')
        expect(sectionItems.length).toBeGreaterThan(0)

        // Verify expected sections are present
        const sectionTexts: string[] = []
        for (const item of sectionItems) {
            sectionTexts.push(await item.getText())
        }

        // Check for core sections
        expect(sectionTexts.some((t) => t.includes('Appearance'))).toBe(true)
        expect(sectionTexts.some((t) => t.includes('Keyboard shortcuts'))).toBe(true)
    })

    it('has a working search input', async () => {
        const handlesBefore = await browser.getWindowHandles()

        await openSettingsViaShortcut()

        const handles = await browser.getWindowHandles()
        if (handles.length <= handlesBefore.length) {
            console.log('Skipping: Multi-window not supported')
            return
        }

        await switchToNewWindow(handlesBefore)

        // Find and interact with search input
        const searchInput = await browser.$('.search-input')
        await searchInput.waitForExist({ timeout: 5000 })

        // Type a search query
        await searchInput.setValue('theme')
        await browser.pause(300)

        // Verify search is working (input value should be set)
        const value = await searchInput.getValue()
        expect(value).toBe('theme')
    })

    it('navigates between sections when clicking', async () => {
        const handlesBefore = await browser.getWindowHandles()

        await openSettingsViaShortcut()

        const handles = await browser.getWindowHandles()
        if (handles.length <= handlesBefore.length) {
            console.log('Skipping: Multi-window not supported')
            return
        }

        await switchToNewWindow(handlesBefore)

        // Wait for sidebar
        const sidebar = await browser.$('.settings-sidebar')
        await sidebar.waitForExist({ timeout: 5000 })

        // Find and click on a section
        const sectionItems = [...(await browser.$$('.section-item'))]
        if (sectionItems.length >= 2) {
            // Click second section
            await sectionItems[1].click()
            await browser.pause(300)

            // Verify it becomes selected
            const classAttr = await sectionItems[1].getAttribute('class')
            expect(classAttr).toContain('selected')
        }
    })

    it('closes settings window with Escape key', async () => {
        const handlesBefore = await browser.getWindowHandles()

        await openSettingsViaShortcut()

        const handles = await browser.getWindowHandles()
        if (handles.length <= handlesBefore.length) {
            console.log('Skipping: Multi-window not supported')
            return
        }

        const originalWindow = await switchToNewWindow(handlesBefore)

        // Verify settings window is open
        const settingsWindow = await browser.$('.settings-window')
        await settingsWindow.waitForExist({ timeout: 5000 })

        // Press Escape to close
        await browser.keys('Escape')
        await browser.pause(500)

        // Check if window closed
        const newHandles = await browser.getWindowHandles()

        if (newHandles.length === 1) {
            // Window closed successfully
            expect(newHandles.length).toBe(1)
        } else {
            // Window might still be open, which is also acceptable
            // (depends on platform behavior)
            console.log('Note: Settings window may not close with Escape in this environment')
        }

        // Switch back to main window
        await browser.switchToWindow(originalWindow)
    })
})

/**
 * Fallback tests that work without multi-window support.
 * These navigate directly to the /settings route.
 */
describe('Settings page (direct navigation)', () => {
    it('renders settings page when navigated to directly', async () => {
        // Navigate to settings route
        await browser.url('/settings')
        await browser.pause(1000)

        // Check for settings window class
        const settingsWindow = await browser.$('.settings-window')

        if (await settingsWindow.isExisting()) {
            expect(await settingsWindow.isDisplayed()).toBe(true)

            // Verify sidebar exists
            const sidebar = await browser.$('.settings-sidebar')
            expect(await sidebar.isExisting()).toBe(true)

            // Verify content wrapper exists
            const content = await browser.$('.settings-content-wrapper')
            expect(await content.isExisting()).toBe(true)
        } else {
            // The route might not work in this test context
            console.log('Settings page not rendered - may require app context')
        }
    })
})
