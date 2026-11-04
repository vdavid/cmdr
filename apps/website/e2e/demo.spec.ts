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

/** The first pane's first folder row (not `..`), once the listing has landed. */
async function firstFolderRow(page: Page) {
  await page.goto('/demo/')
  const pane = page.locator('.file-pane').first()
  await expect(pane.locator('.file-entry').nth(1)).toBeVisible({ timeout: 30_000 })
  const name = await pane
    .locator('.file-entry[data-drop-target-path]:not([data-filename=".."])')
    .first()
    .getAttribute('data-filename')
  if (!name) throw new Error('the demo home folder has no folder to open')
  return { pane, name, row: pane.locator(`.file-entry[data-filename="${name}"]`) }
}

test.describe('Browser demo on a touch screen', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 1280, height: 800 } })

  test('opens a folder on a double tap', async ({ page }) => {
    const { pane, name, row } = await firstFolderRow(page)
    const box = await row.boundingBox()
    if (!box) throw new Error('the folder row has no box')
    await page.touchscreen.tap(box.x + 20, box.y + box.height / 2)
    await page.touchscreen.tap(box.x + 20, box.y + box.height / 2)
    await expect(pane.locator(`.file-entry[data-filename="${name}"]`)).toHaveCount(0)
  })

  // Chrome's DevTools device mode (and other touch browsers) turn a double tap into two plain
  // clicks with no `dblclick` (Leaflet hit the same: github.com/Leaflet/Leaflet/issues/6777).
  // This replays that event stream, since no Playwright input produces it.
  test('opens a folder on a double tap that brings no dblclick', async ({ page }) => {
    const { pane, name, row } = await firstFolderRow(page)
    for (let tap = 0; tap < 2; tap++) {
      await row.dispatchEvent('pointerdown', { pointerType: 'touch', isPrimary: true, button: 0 })
      await row.dispatchEvent('pointerup', { pointerType: 'touch', isPrimary: true, button: 0 })
      await row.dispatchEvent('mousedown', { button: 0, detail: 1 })
      await row.dispatchEvent('mouseup', { button: 0, detail: 1 })
      await row.dispatchEvent('click', { button: 0, detail: 1 })
    }
    await expect(pane.locator(`.file-entry[data-filename="${name}"]`)).toHaveCount(0)
  })

  test('moves the cursor on a single tap', async ({ page }) => {
    const { row } = await firstFolderRow(page)
    const box = await row.boundingBox()
    if (!box) throw new Error('the folder row has no box')
    await page.touchscreen.tap(box.x + 20, box.y + box.height / 2)
    await expect(row).toHaveClass(/is-under-cursor/)
  })
})
