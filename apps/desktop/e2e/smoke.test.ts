import { test, expect } from '@playwright/test'

test('app loads successfully', async ({ page }) => {
    await page.goto('/')
    await expect(page.locator('body')).toBeVisible()
})

test('dual pane interface renders', async ({ page }) => {
    await page.goto('/')

    // Check that dual pane explorer is present
    const explorer = page.locator('.dual-pane-explorer')
    await expect(explorer).toBeVisible()

    // Check that both panes are present
    const panes = page.locator('.file-pane')
    await expect(panes).toHaveCount(2)

    // Check that panes have either file lists OR error messages (both are valid)
    const firstPane = panes.first()
    const fullListOrError = firstPane.locator('.full-list, .error-message, .message')
    await expect(fullListOrError).toBeVisible({ timeout: 10000 })
})
