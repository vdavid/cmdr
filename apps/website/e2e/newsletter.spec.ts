import { test, expect } from '@playwright/test'

test.describe('Newsletter signup', () => {
    test.describe('Header panel', () => {
        test('Newsletter button toggles the panel', async ({ page }) => {
            await page.goto('/')

            const toggle = page.getByRole('button', { name: 'Newsletter' })
            const panel = page.locator('#newsletter-panel')

            // Panel starts hidden
            await expect(panel).toHaveAttribute('aria-hidden', 'true')

            // Click opens it
            await toggle.click()
            await expect(panel).toHaveAttribute('aria-hidden', 'false')
            await expect(toggle).toHaveAttribute('aria-expanded', 'true')

            // Click closes it
            await toggle.click()
            await expect(panel).toHaveAttribute('aria-hidden', 'true')
            await expect(toggle).toHaveAttribute('aria-expanded', 'false')
        })

        test('Escape closes the panel', async ({ page }) => {
            await page.goto('/')

            const toggle = page.getByRole('button', { name: 'Newsletter' })
            const panel = page.locator('#newsletter-panel')

            await toggle.click()
            await expect(panel).toHaveAttribute('aria-hidden', 'false')

            await page.keyboard.press('Escape')
            await expect(panel).toHaveAttribute('aria-hidden', 'true')
        })

        test('panel has an email input and sign up button', async ({ page }) => {
            await page.goto('/')

            const toggle = page.getByRole('button', { name: 'Newsletter' })
            await toggle.click()

            const panel = page.locator('#newsletter-panel')
            await expect(panel.locator('input[type="email"]')).toBeVisible()
            await expect(panel.getByRole('button', { name: 'Sign up' })).toBeVisible()
        })

        test('Newsletter button is hidden when dismissed', async ({ page }) => {
            await page.goto('/')
            await page.evaluate(() => localStorage.setItem('newsletter-dismissed', 'true'))
            await page.reload()

            const toggle = page.locator('[data-newsletter-toggle]')
            await expect(toggle).toBeHidden()
        })

        test('Newsletter button is hidden when subscribed', async ({ page }) => {
            await page.goto('/')
            await page.evaluate(() => localStorage.setItem('newsletter-subscribed', 'true'))
            await page.reload()

            const toggle = page.locator('[data-newsletter-toggle]')
            await expect(toggle).toBeHidden()
        })

        test('Newsletter button is hidden with legacy dismissed key', async ({ page }) => {
            await page.goto('/')
            await page.evaluate(() => localStorage.setItem('newsletter-cta-dismissed', 'true'))
            await page.reload()

            const toggle = page.locator('[data-newsletter-toggle]')
            await expect(toggle).toBeHidden()
        })
    })

    test.describe('Footer form', () => {
        test('has newsletter signup in footer', async ({ page }) => {
            await page.goto('/')

            const footer = page.locator('footer')
            await expect(footer.getByText('Stay in the loop')).toBeVisible()
            await expect(footer.locator('input[type="email"]')).toBeVisible()
            await expect(footer.getByRole('button', { name: 'Sign up' })).toBeVisible()
        })

        test('footer newsletter is visible even when dismissed', async ({ page }) => {
            await page.goto('/')
            await page.evaluate(() => localStorage.setItem('newsletter-dismissed', 'true'))
            await page.reload()

            const footer = page.locator('footer')
            await expect(footer.getByText('Stay in the loop')).toBeVisible()
            await expect(footer.locator('input[type="email"]')).toBeVisible()
        })
    })

    test.describe('Download section form', () => {
        test('has newsletter signup in download section', async ({ page }) => {
            await page.goto('/')

            const download = page.locator('#download')
            await expect(download.getByText(/get notified when they're ready/i)).toBeVisible()
            await expect(download.locator('input[type="email"]')).toBeVisible()
            await expect(download.getByRole('button', { name: 'Sign up' })).toBeVisible()
        })

        test('download section has "Not interested" dismiss link', async ({ page }) => {
            await page.goto('/')

            const download = page.locator('#download')
            const dismissBtn = download.locator('[data-newsletter-inline-dismiss]')
            await expect(dismissBtn).toBeVisible()
            await expect(dismissBtn).toHaveText('Not interested')
        })

        test('download section hides when dismissed', async ({ page }) => {
            await page.goto('/')
            await page.evaluate(() => localStorage.setItem('newsletter-dismissed', 'true'))
            await page.reload()

            const download = page.locator('#download')
            const content = download.locator('[data-newsletter-inline-content]')
            await expect(content).toBeHidden()
        })

        test('download section shows "You\'re on the list" when subscribed', async ({ page }) => {
            await page.goto('/')
            await page.evaluate(() => localStorage.setItem('newsletter-subscribed', 'true'))
            await page.reload()

            const download = page.locator('#download')
            await expect(download.locator('[data-newsletter-inline-subscribed]')).toBeVisible()
            await expect(download.locator('[data-newsletter-inline-content]')).toBeHidden()
        })
    })

    test.describe('Client-side validation', () => {
        test('shows error for empty email', async ({ page }) => {
            await page.goto('/')

            // Use the footer form (always visible)
            const footer = page.locator('footer')
            const submitButton = footer.getByRole('button', { name: 'Sign up' })
            await submitButton.click()

            await expect(footer.getByText('Enter a valid email address.')).toBeVisible()
        })

        test('shows error for invalid email', async ({ page }) => {
            await page.goto('/')

            const footer = page.locator('footer')
            await footer.locator('input[type="email"]').fill('not-an-email')
            await footer.getByRole('button', { name: 'Sign up' }).click()

            await expect(footer.getByText('Enter a valid email address.')).toBeVisible()
        })
    })

    test.describe('Accessibility', () => {
        test('email inputs have associated labels', async ({ page }) => {
            await page.goto('/')

            const emailInputs = await page.locator('[data-newsletter-form] input[type="email"]').all()
            expect(emailInputs.length).toBeGreaterThanOrEqual(2) // Footer + Download

            for (const input of emailInputs) {
                const id = await input.getAttribute('id')
                expect(id).toBeTruthy()
                const label = page.locator(`label[for="${id}"]`)
                await expect(label).toBeAttached()
            }
        })

        test('feedback areas have aria-live', async ({ page }) => {
            await page.goto('/')

            const feedbacks = await page.locator('[data-newsletter-form] [aria-live="polite"]').all()
            expect(feedbacks.length).toBeGreaterThanOrEqual(2)
        })

        test('honeypot is hidden from screen readers', async ({ page }) => {
            await page.goto('/')

            const honeypots = await page.locator('[data-newsletter-form] [aria-hidden="true"]').all()
            expect(honeypots.length).toBeGreaterThanOrEqual(2)
        })
    })
})
