import { test, expect, type Page } from '@playwright/test'
import { existsSync } from 'node:fs'

// The browser demo at /demo/ (issue #302): the real desktop frontend on a fake backend, copied into
// dist/ only by a `CMDR_WEBSITE_DEMO=1` build (src/plugins/demo.ts). This is the guard that an IPC
// change in the desktop app can't silently break it. CI's website job always builds it, so there a
// missing demo fails the run; a plain local `pnpm build` skips it.
const demoBuilt = existsSync(new URL('../dist/demo/index.html', import.meta.url))
test.skip(
  !demoBuilt && !process.env.CI,
  'dist/demo/ isn’t built: run `pnpm build:demo` at the repo root, then `CMDR_WEBSITE_DEMO=1 pnpm build` here',
)

/** Collects every uncaught exception and unhandled rejection the page throws. */
function collectPageErrors(page: Page): Error[] {
  const errors: Error[] = []
  page.on('pageerror', (error) => errors.push(error))
  return errors
}

test.describe('Browser demo', () => {
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
