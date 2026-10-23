import { test, expect } from '@playwright/test'

test.describe('Homepage', () => {
  test('has correct title', async ({ page }) => {
    await page.goto('/')
    await expect(page).toHaveTitle(/Cmdr/i)
  })

  test('displays hero section with main heading', async ({ page }) => {
    await page.goto('/')

    // Check for main heading - matches actual site content
    const heading = page.locator('h1')
    await expect(heading).toBeVisible()
    await expect(heading).toContainText(/file manager/i)
  })

  test('has download CTA button', async ({ page }) => {
    await page.goto('/')

    // Look for at least one download button - use first() to avoid strict mode violation
    const ctaButton = page.getByRole('link', { name: /download/i }).first()
    await expect(ctaButton).toBeVisible()
  })

  test('displays pricing information', async ({ page }) => {
    await page.goto('/')

    // Check for pricing mention ($59 once, or free for personal use)
    const pageContent = await page.textContent('body')
    expect(pageContent).toMatch(/\$59|free.*personal/i)
  })

  test('every landing clip renders as a video, not a placeholder', async ({ page }) => {
    // A `public/clips/clips.json` shape the build can't read turns every clip into a placeholder
    // card without failing anything, so this is the guard for the manifest contract.
    await page.goto('/')
    await expect(page.locator('.clip-placeholder')).toHaveCount(0)
    const clips = page.locator('[data-product-clip]')
    await expect(clips).toHaveCount(6)
    for (const description of await clips.locator('[data-clip-description]').all()) {
      await expect(description).toHaveText(/\w/)
    }
  })

  test('has navigation header', async ({ page }) => {
    await page.goto('/')

    // Check for essential nav elements
    const header = page.locator('header')
    await expect(header).toBeVisible()
  })

  test('is accessible (basic checks)', async ({ page }) => {
    await page.goto('/')

    // Check for alt text on images (if any exist)
    const images = await page.locator('img').all()
    for (const img of images) {
      const alt = await img.getAttribute('alt')
      expect(alt).not.toBeNull()
    }

    // Check for proper heading hierarchy (h1 exists)
    const h1Count = await page.locator('h1').count()
    expect(h1Count).toBeGreaterThanOrEqual(1)
  })
})
