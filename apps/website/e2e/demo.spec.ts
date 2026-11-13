import { test, expect, type Locator, type Page } from '@playwright/test'
import { collectPageErrors, skipWithoutDemo, visitorOf } from './demo-support.js'

// The full demo suite: the `demo` and `demo-mac` Playwright projects (a Windows visitor and a Mac
// one; `visitorOf` says which keys and words each gets), run against a demo built from the same sources
// (the release pipeline's `demo` job, the nightly CI run), never the website lane's released image.
// Where each suite runs: `demo-support.ts`. Boot, indexing, and the first listing: `demo-smoke.spec.ts`.
skipWithoutDemo()

test.describe('Browser demo', () => {
  // ⌘F runs the real search core in the WASM module (crates/cmdr-listing-wasm, `search.rs`) over the
  // volume the panes list. Enter (Go to file) lands the pane's cursor on the found file.
  test('finds a file with ⌘F and Enter lands the cursor on it', async ({ page }) => {
    const errors = collectPageErrors(page)
    await page.goto('/demo/')
    const pane = page.locator('.file-pane').first()
    await expect(pane.locator('.file-entry').nth(1)).toBeVisible({ timeout: 30_000 })
    const dialog = page.getByRole('dialog')
    // Focus takes a moment to land on a pane after boot.
    const { mod } = await visitorOf(page)
    await expect(async () => {
      if (!(await dialog.isVisible())) await page.keyboard.press(`${mod}+f`)
      await expect(dialog).toBeVisible({ timeout: 1_000 })
    }).toPass({ timeout: 10_000 })

    await page.keyboard.type('annual report')
    await expect(dialog.getByText('Annual report.pdf')).toBeVisible()
    await expect(dialog.getByText('1 of 1 result').first()).toBeVisible()
    await page.keyboard.press('Enter')

    await expect(dialog).toHaveCount(0)
    await expect(pane.locator('.file-entry[data-filename="Annual report.pdf"]')).toHaveClass(/is-under-cursor/)
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
     * back on a pane, and a key pressed before that goes nowhere, as in the app.
     */
    async function pressUntil(page: Page, key: string, opened: Locator) {
      await expect(async () => {
        if (!(await opened.isVisible())) await page.keyboard.press(key)
        await expect(opened).toBeVisible({ timeout: 1_000 })
      }).toPass({ timeout: 10_000 })
    }

    async function openDemo(page: Page, search = '') {
      // A secondary window is an iframe of its route (`/demo/queue`). nginx answers any path under
      // `/demo/` with the app page (the SPA fallback, `nginx.conf`); the static server here
      // doesn't, so this does what nginx does.
      await page.route(/\/demo\/(queue|settings|shortcuts)\/?$/, (route) =>
        route.fulfill({ path: new URL('../dist/demo/index.html', import.meta.url).pathname }),
      )
      await page.goto(`/demo/${search}`)
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
      await pressUntil(page, 'F8', page.getByText((await visitorOf(page)).noTrashWarning))
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
      // Regression: at the demo's 125% zoom, the "Still running in the background" toast covered the chip and took its
      // click. The toast stack starts below the status corner's real box (`status-corner/toast-clearance.ts`).
      const chip = page.getByRole('button', { name: /Open the operation queue/ })
      const toast = page.locator('[data-toast-frame]').filter({ hasText: 'Still running in the background' })
      await expect(toast).toBeVisible()
      // Polled: the stack moves down a frame after the chip shows, when the corner's resize lands.
      await expect
        .poll(async () => {
          const [chipBox, toastBox] = await Promise.all([chip.boundingBox(), toast.boundingBox()])
          return chipBox && toastBox ? toastBox.y - (chipBox.y + chipBox.height) : -1
        })
        .toBeGreaterThanOrEqual(0)
      // The status corner's progress opens the queue, as in the app.
      await chip.click()
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

    /**
     * The rolled-back operation's state in the Operation log's list. The picked operation's detail
     * header badges the same state, by design (`apps/desktop/src/lib/operation-log/DETAILS.md`), so
     * an unscoped "Rolled back" matches twice.
     */
    const rolledBackInList = (page: Page) =>
      page.getByRole('grid', { name: 'Operations' }).getByText('Rolled back', { exact: true })

    test('the operation log rolls a copy back', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      await page.keyboard.press('End')
      await page.keyboard.press('F5')
      await confirmTransfer(page, 'Copy')
      await expect(row(right(page), 'notes')).toBeVisible()
      await pressUntil(page, (await visitorOf(page)).operationLogKey, page.getByText('Operation log', { exact: true }))
      await page.getByRole('button', { name: 'Roll back', exact: true }).click()
      const confirm = page.locator('[role=dialog], [role=alertdialog]').filter({ hasText: 'Roll this operation back?' })
      await confirm.getByRole('button', { name: 'Roll back', exact: true }).click()
      await expect(rolledBackInList(page)).toBeVisible()
      await expect(row(right(page), 'notes')).toHaveCount(0)
      expect(errors.map((error) => error.message)).toEqual([])
    })

    // ⌃M runs the real Multi-Rename engine in the WASM module (crates/cmdr-listing-wasm, `multi_rename.rs`) over
    // the panes and the write engine, so a rename lands in the pane and the operation log rolls it back.
    const multiRename = (page: Page) => page.getByRole('dialog').filter({ hasText: 'Multi-rename' })

    /** Opens the folder `name` in the left pane, then waits for its first row. */
    async function enterFolder(page: Page, name: string, firstRow: string) {
      await row(left(page), name).dblclick()
      await expect(row(left(page), firstRow)).toBeVisible()
    }

    /** Types `mask` over the sheet's name mask. */
    async function typeMask(page: Page, mask: string) {
      const field = multiRename(page).getByRole('textbox', { name: 'Name mask' })
      await field.fill(mask)
    }

    /** The demo's commands nobody answered, by name (`apps/desktop/src/lib/demo/unanswered.ts`). */
    async function unanswered(page: Page): Promise<string[]> {
      return page.evaluate(() => [
        ...(window as unknown as { __cmdrDemo: { unanswered: Map<string, number> } }).__cmdrDemo.unanswered.keys(),
      ])
    }

    test('Multi-Rename renames a selection, and the operation log rolls it back', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      await enterFolder(page, 'Documents', 'Annual report')
      // Select `Annual report.pdf` and `Budget 2026.xlsx` (Insert toggles a row and moves down).
      await row(left(page), 'Annual report').click()
      await page.keyboard.press('Insert')
      await page.keyboard.press('Insert')
      await pressUntil(page, 'Control+m', multiRename(page))
      await typeMask(page, '[N]_[C:2]')
      await expect(multiRename(page).getByText('Annual report_01.pdf')).toBeVisible()
      await expect(multiRename(page).getByText('Budget 2026_02.xlsx')).toBeVisible()
      await multiRename(page)
        .getByRole('button', { name: /^Rename 2 files/ })
        .click()
      await expect(row(left(page), 'Annual report_01')).toBeVisible()
      await expect(row(left(page), 'Budget 2026_02')).toBeVisible()

      await pressUntil(page, (await visitorOf(page)).operationLogKey, page.getByText('Operation log', { exact: true }))
      await page.getByRole('button', { name: 'Roll back', exact: true }).first().click()
      const confirm = page.locator('[role=dialog], [role=alertdialog]').filter({ hasText: 'Roll this operation back?' })
      await confirm.getByRole('button', { name: 'Roll back', exact: true }).click()
      await expect(rolledBackInList(page)).toBeVisible()
      await expect(row(left(page), 'Annual report_01')).toHaveCount(0)
      await expect(row(left(page), 'Annual report')).toBeVisible()
      expect((await unanswered(page)).filter((name) => name.includes('multi_rename'))).toEqual([])
      expect(errors.map((error) => error.message)).toEqual([])
    })

    test('Multi-Rename says a photo’s fields can’t be read, and Results needs the desktop app', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      await enterFolder(page, 'Desktop', 'todo')
      await row(left(page), 'Screenshot').click()
      await pressUntil(page, 'Control+m', multiRename(page))
      await typeMask(page, '[=exif.taken]')
      await expect(multiRename(page).getByLabel('Can’t read tags')).toBeVisible()

      const results = multiRename(page).getByRole('button', { name: /^Results…/ })
      await expect(results).toHaveAttribute('aria-disabled', 'true')
      await results.focus()
      await expect(page.getByText('Results needs the Cmdr desktop app')).toBeVisible()
      expect((await unanswered(page)).filter((name) => name.includes('multi_rename'))).toEqual([])
      expect(errors.map((error) => error.message)).toEqual([])
    })

    /**
     * Drops files from the computer onto `target`, as a browser hands a Finder drag to the page: a
     * `DataTransfer` of `File`s on `dragenter`, `dragover`, and `drop` (apps/desktop/src/lib/demo/os-drop.ts).
     */
    async function dropFromComputer(
      page: Page,
      target: Locator,
      files: { name: string; text?: string; bytes?: number }[],
    ) {
      const box = await target.boundingBox()
      if (!box) throw new Error('the drop target has no box')
      await page.evaluate(
        ({ files, x, y }) => {
          const dataTransfer = new DataTransfer()
          for (const f of files) dataTransfer.items.add(new File([f.text ?? new Uint8Array(f.bytes ?? 0)], f.name))
          const init = { clientX: x, clientY: y, bubbles: true, cancelable: true, dataTransfer }
          const element = document.elementFromPoint(x, y) ?? document.body
          for (const type of ['dragenter', 'dragover', 'drop']) element.dispatchEvent(new DragEvent(type, init))
        },
        // Upper left: a toast from an earlier transfer sits over the lower right, and a drop on it
        // lands nowhere, as on desktop.
        { files, x: box.x + box.width / 4, y: box.y + box.height / 3 },
      )
    }

    test('files dropped in from the computer land through the transfer dialog, conflicts included', async ({
      page,
    }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      const name = 'from my computer.txt'
      await dropFromComputer(page, right(page), [{ name, text: 'hello from outside' }])
      await confirmTransfer(page, 'Move')
      await expect(row(right(page), 'from my computer')).toBeVisible()

      // The same name again: the dialog sees the clash, as for any transfer.
      // The progress dialog closes a moment after the row lands; a drop onto it lands nowhere.
      await expect(page.getByRole('dialog')).toHaveCount(0)
      await dropFromComputer(page, right(page), [{ name, text: 'again' }])
      await expect(page.getByText('1 file already exists')).toBeVisible()
      expect(errors.map((error) => error.message)).toEqual([])
    })

    test('a drop past the visit’s 10 MB says what stayed out', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      await dropFromComputer(page, right(page), [{ name: 'huge.bin', bytes: 11_000_000 }])
      await expect(page.getByText(/holds up to 10 MB of your files per visit, so it left out 1 item/)).toBeVisible()
      await expect(row(right(page), 'huge')).toHaveCount(0)
      expect(errors.map((error) => error.message)).toEqual([])
    })

    test('a dismissed drop gives its room back: the full 10 MB comes in again', async ({ page }) => {
      const errors = collectPageErrors(page)
      await openDemo(page)
      const whole = [{ name: 'whole.bin', bytes: 10_000_000 }]
      await dropFromComputer(page, right(page), whole)
      await expect(page.getByText(/^Move 1 file$/)).toBeVisible()
      await page.keyboard.press('Escape')
      await expect(page.getByRole('dialog')).toHaveCount(0)

      await dropFromComputer(page, right(page), whole)
      await expect(page.getByText(/^Move 1 file$/)).toBeVisible()
      await expect(page.getByText(/holds up to 10 MB/)).toHaveCount(0)
      await page.keyboard.press('Enter')
      await expect(row(right(page), 'whole')).toBeVisible()
      expect(errors.map((error) => error.message)).toEqual([])
    })

    test('a copy onto a full disk stays in the queue, and Retry lands it once there is room', async ({ page }) => {
      const errors = collectPageErrors(page)
      // `?free=100` leaves 100 bytes on the disk, too little for `notes.md` (its 372 real bytes).
      await openDemo(page, '?free=100')
      await page.keyboard.press('End')
      await page.keyboard.press('F5')
      await page.getByRole('button', { name: /^Background\b/ }).click()
      await expect(page.getByText('Couldn’t finish copying').first()).toBeVisible({ timeout: 10_000 })
      await expect(row(right(page), 'notes')).toHaveCount(0)

      // Make room: F8 on `Cmdr.dmg` (38 MB) in Downloads, the first file there.
      await page.keyboard.press('Tab')
      await page.keyboard.press('ArrowDown')
      await pressUntil(page, 'F8', page.getByText((await visitorOf(page)).noTrashWarning))
      await page.keyboard.press('Enter')
      await expect(row(right(page), 'Cmdr')).toHaveCount(0)

      // The queue kept the failed copy; its Retry lands it now.
      await page.getByRole('button', { name: 'Show in operation queue' }).first().click()
      const queue = page.frameLocator('.cmdr-demo-window iframe')
      await expect(queue.getByText('Couldn’t finish', { exact: true })).toBeVisible({ timeout: 10_000 })
      await expect(queue.getByText('The destination ran out of space before everything was written.')).toBeVisible()
      await queue.getByRole('button', { name: /^Retry\b/ }).click()
      await expect(row(right(page), 'notes')).toBeVisible()
      await expect(queue.getByText('Couldn’t finish', { exact: true })).toHaveCount(0)
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
