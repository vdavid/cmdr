/**
 * Playwright E2E smoke tests for Cmdr desktop app.
 *
 * These tests run in a browser (Chromium/WebKit), NOT the Tauri webview.
 * They verify basic UI rendering and interactions that don't require Tauri backend.
 *
 * For comprehensive E2E testing with file operations, use the Linux E2E tests:
 *   pnpm test:e2e:linux   (runs in Docker with actual Tauri app via tauri-driver)
 */

import { test, expect } from '@playwright/test'

test.describe('Basic rendering', () => {
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
    })
})

test.describe('Pane interactions', () => {
    test('Tab switches focus between panes', async ({ page }) => {
        await page.goto('/')

        // Wait for panes to load
        const panes = page.locator('.file-pane')
        await expect(panes.first()).toBeVisible({ timeout: 10000 })

        // Initially left pane should be focused
        const leftPane = panes.first()
        await expect(leftPane).toHaveClass(/is-focused/)

        // Press Tab to switch to right pane
        await page.keyboard.press('Tab')

        // Now right pane should be focused
        const rightPane = panes.nth(1)
        await expect(rightPane).toHaveClass(/is-focused/)
        await expect(leftPane).not.toHaveClass(/is-focused/)

        // Press Tab again to switch back to left pane
        await page.keyboard.press('Tab')
        await expect(leftPane).toHaveClass(/is-focused/)
        await expect(rightPane).not.toHaveClass(/is-focused/)
    })

    test('clicking on other pane switches focus', async ({ page }) => {
        await page.goto('/')

        // Wait for panes to load
        const panes = page.locator('.file-pane')
        await expect(panes.first()).toBeVisible({ timeout: 10000 })

        // Initially left pane should be focused
        const leftPane = panes.first()
        const rightPane = panes.nth(1)
        await expect(leftPane).toHaveClass(/is-focused/)

        // Click on right pane
        await rightPane.click()

        // Right pane should now be focused
        await expect(rightPane).toHaveClass(/is-focused/)
        await expect(leftPane).not.toHaveClass(/is-focused/)
    })
})
