/**
 * E2E tests for the settings page on Linux.
 *
 * Uses SvelteKit's link-click interception for client-side navigation.
 * browser.url() doesn't work in Tauri (navigates to about:blank with DNS error),
 * and pushState+popstate doesn't trigger SvelteKit's router. Creating a temporary
 * <a> element and clicking it does trigger SvelteKit's client-side navigation.
 */

/** Navigate to a SvelteKit route via link-click interception. */
async function navigateToRoute(path: string): Promise<void> {
    await browser.execute((p: string) => {
        const a = document.createElement('a')
        a.href = p
        document.body.appendChild(a)
        a.click()
        a.remove()
    }, path)
}

describe('Settings page', () => {
    before(async () => {
        // Wait for the main app to fully load first
        const explorer = browser.$('.dual-pane-explorer')
        await explorer.waitForExist({ timeout: 15000 })

        // Navigate to settings via SvelteKit client-side routing
        await navigateToRoute('/settings')

        // Wait for settings to fully initialize (loads from Tauri store)
        const settingsWindow = browser.$('.settings-window')
        await settingsWindow.waitForExist({ timeout: 15000 })

        // Wait for initialization to complete (sidebar appears when initialized=true)
        const sidebar = browser.$('.settings-sidebar')
        await sidebar.waitForExist({ timeout: 10000 })
    })

    it('renders the settings page', async () => {
        const settingsWindow = browser.$('.settings-window')
        expect(await settingsWindow.isExisting()).toBe(true)

        const layout = browser.$('.settings-layout')
        expect(await layout.isExisting()).toBe(true)
    })

    it('displays sidebar with sections', async () => {
        const sidebar = browser.$('.settings-sidebar')
        expect(await sidebar.isExisting()).toBe(true)

        const sectionItems = await browser.$$('.section-item')
        expect(sectionItems.length).toBeGreaterThan(0)
    })

    it('shows expected sections like Appearance and Keyboard shortcuts', async () => {
        const sectionItems = await browser.$$('.section-item')
        const sectionTexts: string[] = []
        for (const item of sectionItems) {
            sectionTexts.push(await item.getText())
        }

        expect(sectionTexts.some((t) => t.includes('Appearance'))).toBe(true)
        expect(sectionTexts.some((t) => t.includes('Keyboard shortcuts'))).toBe(true)
    })

    it('has a working search input', async () => {
        const searchInput = browser.$('.search-input')
        await searchInput.waitForExist({ timeout: 5000 })

        await searchInput.setValue('theme')
        await browser.pause(300)

        const value = await searchInput.getValue()
        expect(value).toBe('theme')

        // Clear search via JS — WebKitGTK's clearValue() doesn't fire oninput,
        // and setValue('') fails with "Missing text parameter"
        await browser.execute(() => {
            const input = document.querySelector('.search-input') as HTMLInputElement
            if (input) {
                input.value = ''
                input.dispatchEvent(new Event('input', { bubbles: true }))
            }
        })
        await browser.pause(500)
    })

    it('navigates between sections when clicking', async () => {
        const sidebar = browser.$('.settings-sidebar')
        await sidebar.waitForExist({ timeout: 5000 })

        // Wait for section items to fully render (search clear in previous test
        // must complete for all sections to reappear)
        await browser.waitUntil(
            async () => [...(await browser.$$('.section-item'))].length >= 2,
            { timeout: 10000, timeoutMsg: 'Expected at least 2 section items in sidebar' },
        )
        const sectionItems = [...(await browser.$$('.section-item'))]

        // Click second section via JS (WebKitGTK may reject native clicks on non-form elements)
        await browser.execute((el: HTMLElement) => el.click(), sectionItems[1] as unknown as HTMLElement)
        await browser.pause(300)

        const classAttr = await sectionItems[1].getAttribute('class')
        expect(classAttr).toContain('selected')
    })
})
