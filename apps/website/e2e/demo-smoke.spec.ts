import { test, expect } from '@playwright/test'
import { collectPageErrors, skipWithoutDemo } from './demo-support.js'

// The demo smoke: only what every release's demo answers, since the website lane runs it against
// the released demo image (`demo-support.ts` says where the full suite runs).
skipWithoutDemo()

test.describe('Browser demo smoke', () => {
  test('boots the app without a page error', async ({ page }) => {
    const errors = collectPageErrors(page)
    await page.goto('/demo/')
    await expect(page).toHaveTitle('Cmdr demo')
    await expect(page.locator('.file-pane').first()).toBeVisible({ timeout: 30_000 })
    expect(errors.map((error) => error.message)).toEqual([])
  })

  test('is kept out of search results', async ({ page }) => {
    await page.goto('/demo/')
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute('content', /noindex/)
  })

  // The real listing engine (crates/cmdr-listing-wasm, loaded by apps/desktop/src/lib/demo/listing.ts)
  // answers the pane: a missing `.wasm`, a CSP block, or a wire mismatch leaves it on `..` alone.
  test('lists the home folder in a pane', async ({ page }) => {
    const errors = collectPageErrors(page)
    await page.goto('/demo/')
    // More than one row, so the `..` row alone doesn't count as a listing.
    await expect(page.locator('.file-pane').first().locator('.file-entry').nth(1)).toBeVisible({ timeout: 30_000 })
    expect(errors.map((error) => error.message)).toEqual([])
  })
})
