import { test, expect } from '@playwright/test'
import { DEMO_DESCRIPTION, DEMO_TITLE } from '../src/plugins/demo.ts'
import { collectPageErrors, skipWithoutDemo } from './demo-support.js'

// The demo smoke: only what every release's demo answers, since the website lane runs it against
// the released demo image (`demo-support.ts` says where the full suite runs). The page's `<head>` and
// the sitemap come from this site's build (`src/plugins/demo.ts`), not the image, so they're safe here.
skipWithoutDemo()

test.describe('Browser demo smoke', () => {
  test('boots the app without a page error', async ({ page }) => {
    const errors = collectPageErrors(page)
    await page.goto('/demo/')
    await expect(page).toHaveTitle(DEMO_TITLE)
    await expect(page.locator('.file-pane').first()).toBeVisible({ timeout: 30_000 })
    expect(errors.map((error) => error.message)).toEqual([])
  })

  // A real page: indexable, described, and canonical at `/demo/`, so the app's own routes (served the
  // same page by the SPA fallback, and `noindex` from nginx, which `website-docker-build` probes)
  // fold into it.
  test('is a page search engines can list', async ({ page }) => {
    await page.goto('/demo/')
    await expect(page.locator('meta[name="robots"]')).toHaveCount(0)
    await expect(page.locator('meta[name="description"]')).toHaveAttribute('content', DEMO_DESCRIPTION)
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute('href', 'https://getcmdr.com/demo/')
    await expect(page.locator('meta[property="og:title"]')).toHaveAttribute('content', DEMO_TITLE)
  })

  test('is in the sitemap, alone of everything under /demo/', async ({ request }) => {
    const sitemap = await (await request.get('/sitemap-0.xml')).text()
    const demoUrls = [...sitemap.matchAll(/<loc>([^<]*\/demo\/[^<]*)<\/loc>/g)].map((match) => match[1])
    expect(demoUrls).toEqual(['https://getcmdr.com/demo/'])
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
