import { test, expect, type Locator, type Page } from '@playwright/test'
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

  // File operations run through the real write engine in the WASM module (crates/cmdr-listing-wasm,
  // `operations_host/`), on the in-memory volume both panes list, so each lands in the other pane.
  test.describe('file operations', () => {
    const left = (page: Page) => page.locator('.file-pane').nth(0)
    const right = (page: Page) => page.locator('.file-pane').nth(1)
    const row = (pane: ReturnType<typeof left>, name: string) =>
      pane.locator('.file-entry').filter({ hasText: new RegExp(`^\\s*${name}\\b`) })

    /** Waits for the transfer dialog, then confirms it with Enter, as a keyboard user does. */
    async function confirmTransfer(page: Page, verb: 'Copy' | 'Move') {
      await expect(page.getByText(new RegExp(`^${verb} \\d+ files?$`))).toBeVisible()
      await page.keyboard.press('Enter')
    }

    /**
     * Presses `key` until `opened` shows. Right after a dialog closes, focus takes a moment to land
     * back on a pane, and a key pressed before that goes nowhere, as in the app. A `⌘` shortcut
     * tries Meta and then Control: the demo reads ⌘ as Ctrl off a Mac, and the emulated device
     * isn't one, while the host running the browser may be.
     */
    async function pressUntil(page: Page, key: string, opened: Locator) {
      const keys = key.startsWith('Meta+') ? [key, key.replace('Meta+', 'Control+')] : [key]
      let attempt = 0
      await expect(async () => {
        if (!(await opened.isVisible())) await page.keyboard.press(keys[attempt++ % keys.length])
        await expect(opened).toBeVisible({ timeout: 1_000 })
      }).toPass({ timeout: 10_000 })
    }

    async function openDemo(page: Page) {
      // A secondary window is an iframe of its route (`/demo/queue`). nginx answers any path under
      // `/demo/` with the app page (the SPA fallback, `nginx.conf`); the static server here
      // doesn't, so this does what nginx does.
      await page.route(/\/demo\/(queue|settings|shortcuts)\/?$/, (route) =>
        route.fulfill({ path: new URL('../dist/demo/index.html', import.meta.url).pathname }),
      )
      await page.goto('/demo/')
      await expect(left(page).locator('.file-entry').nth(1)).toBeVisible({ timeout: 30_000 })
    }

    test('new folder, copy, a conflict answered Skip, rename, move, and delete', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)

      // F7 in the home folder.
      await page.keyboard.press('F7')
      await page.keyboard.type('Work')
      await page.keyboard.press('Enter')
      await expect(row(left(page), 'Work')).toBeVisible()

      // F5 the last file into Downloads (the right pane).
      await page.keyboard.press('End')
      await page.keyboard.press('F5')
      await confirmTransfer(page, 'Copy')
      await expect(row(right(page), 'notes')).toBeVisible()

      // Again: the dialog sees the clash; Ask later, then Skip at the prompt.
      await pressUntil(page, 'F5', page.getByText('1 file already exists'))
      await confirmTransfer(page, 'Copy')
      await expect(page.getByText('File already exists')).toBeVisible()
      await page
        .getByRole('button', { name: /^Skip\b/ })
        .first()
        .click()
      await expect(page.getByText('File already exists')).toBeHidden()

      // Shift+F6 renames in place.
      await page.keyboard.press('Shift+F6')
      await page.keyboard.type('journal')
      await page.keyboard.press('Enter')
      await expect(row(left(page), 'journal')).toBeVisible()

      // F6 moves it to Downloads.
      await page.keyboard.press('F6')
      await confirmTransfer(page, 'Move')
      await expect(row(right(page), 'journal')).toBeVisible()
      await expect(row(left(page), 'journal')).toHaveCount(0)

      // F8 on Work: no Trash on this volume, so the app's permanent-delete confirm.
      // Folders sort first, so with `journal` gone the last row is `Work`.
      await page.keyboard.press('End')
      await pressUntil(page, 'F8', page.getByText('This volume doesn’t support trash.'))
      await page.keyboard.press('Enter')
      await expect(row(left(page), 'Work')).toHaveCount(0)

      expect(errors.map((error) => error.message)).toEqual([])
    })

    test('the operation queue pauses a running copy', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      // `dataset.csv` (120 MB) copies at the demo device's pace, so it's still running.
      await page.keyboard.press('Tab')
      await page.keyboard.press('ArrowDown')
      await page.keyboard.press('ArrowDown')
      await page.keyboard.press('F5')
      await confirmTransfer(page, 'Copy')
      await page.getByRole('button', { name: /^Background\b/ }).click()
      // The status corner's progress opens the queue, as in the app.
      await page.getByRole('button', { name: /Open the operation queue/ }).click()
      const queue = page.frameLocator('.cmdr-demo-window iframe')
      await expect(queue.getByText('Running', { exact: true })).toBeVisible({ timeout: 10_000 })
      await queue
        .getByRole('button', { name: /^Pause\b/ })
        .first()
        .click()
      await expect(queue.getByText('Paused', { exact: true })).toBeVisible()
      await queue
        .getByRole('button', { name: /^Resume\b/ })
        .first()
        .click()
      await expect(queue.getByText('Running', { exact: true })).toBeVisible()
      expect(errors.map((error) => error.message)).toEqual([])
    })

    test('the operation log rolls a copy back', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      await page.keyboard.press('End')
      await page.keyboard.press('F5')
      await confirmTransfer(page, 'Copy')
      await expect(row(right(page), 'notes')).toBeVisible()
      await pressUntil(page, 'Meta+Alt+l', page.getByText('Operation log', { exact: true }))
      await page.getByRole('button', { name: 'Roll back', exact: true }).click()
      const confirm = page.locator('[role=dialog], [role=alertdialog]').filter({ hasText: 'Roll this operation back?' })
      await confirm.getByRole('button', { name: 'Roll back', exact: true }).click()
      await expect(page.getByText('Rolled back', { exact: true })).toBeVisible()
      await expect(row(right(page), 'notes')).toHaveCount(0)
      expect(errors.map((error) => error.message)).toEqual([])
    })
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
