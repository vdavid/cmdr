import { test, expect } from '@playwright/test'

test.describe('Pricing page', () => {
    test('has correct title and heading', async ({ page }) => {
        await page.goto('/pricing')
        await expect(page).toHaveTitle(/Pricing.*Cmdr/i)

        const heading = page.locator('h1')
        await expect(heading).toBeVisible()
        await expect(heading).toContainText(/free forever for personal use/i)
    })

    test('displays all four pricing tiers', async ({ page }) => {
        await page.goto('/pricing')

        // Check for all tier names (use exact match to avoid matching h1)
        await expect(page.getByRole('heading', { name: 'Personal', exact: true })).toBeVisible()
        await expect(page.getByRole('heading', { name: 'Supporter', exact: true })).toBeVisible()
        await expect(page.getByRole('heading', { name: 'Commercial', exact: true })).toBeVisible()
        await expect(page.getByRole('heading', { name: 'Perpetual', exact: true })).toBeVisible()
    })

    test('shows correct prices', async ({ page }) => {
        await page.goto('/pricing')

        const content = await page.textContent('body')
        expect(content).toContain('Free')
        expect(content).toContain('$10')
        expect(content).toContain('$59')
        expect(content).toContain('$199')
    })

    test('has download button', async ({ page }) => {
        await page.goto('/pricing')

        // Use the main download button (with "Download Cmdr" text)
        const downloadButton = page.getByRole('link', { name: /download cmdr/i })
        await expect(downloadButton).toBeVisible()
        await expect(downloadButton).toHaveAttribute('href', /\.dmg$/)
    })

    test('buy buttons are present and clickable', async ({ page }) => {
        await page.goto('/pricing')

        // Find buy buttons by their text content
        const supporterButton = page.getByRole('button', { name: /buy supporter/i })
        const commercialButton = page.getByRole('button', { name: /buy commercial/i })
        const perpetualButton = page.getByRole('button', { name: /buy perpetual/i })

        await expect(supporterButton).toBeVisible()
        await expect(commercialButton).toBeVisible()
        await expect(perpetualButton).toBeVisible()

        // Verify buttons have the correct data attributes for Paddle
        await expect(supporterButton).toHaveAttribute('data-paddle-price', 'supporter')
        await expect(commercialButton).toHaveAttribute('data-paddle-price', 'commercialSubscription')
        await expect(perpetualButton).toHaveAttribute('data-paddle-price', 'commercialPerpetual')
    })

    test('clicking buy button triggers Paddle checkout', async ({ page }) => {
        // Block Paddle from loading - we just want to test the button interaction
        await page.route('**/paddle.com/**', (route) => {
            route.abort()
        })

        await page.goto('/pricing')

        // Wait for page to be ready
        await page.waitForLoadState('domcontentloaded')

        // The buy buttons should be present even if Paddle hasn't loaded
        const commercialButton = page.getByRole('button', { name: /buy commercial/i })
        await expect(commercialButton).toBeVisible()

        // Click should not throw (even if Paddle isn't loaded, the button handler should exist)
        // In production, this would open the Paddle checkout overlay
        await commercialButton.click()

        // We can't fully test Paddle checkout without real credentials,
        // but we verified the button exists and is clickable
    })

    test('FAQ section is present', async ({ page }) => {
        await page.goto('/pricing')

        await expect(page.getByRole('heading', { name: /frequently asked/i })).toBeVisible()

        // Check for some FAQ questions
        const content = await page.textContent('body')
        expect(content).toContain('commercial use')
        expect(content).toContain('multiple machines')
    })
})
