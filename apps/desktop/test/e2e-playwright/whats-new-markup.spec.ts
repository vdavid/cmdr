/**
 * A "What's new" entry keeps its inline markdown in the text flow.
 *
 * Changelog entries carry `code` spans and bold runs (about one entry in twelve
 * has a `code` span), and `snarkdown` renders each as its own element inside the
 * entry's `<li>`. When that `<li>` was a two-column grid, every element became a
 * grid ITEM: the text split across cells and a `<code>` auto-placed into the
 * 1.15em bullet column, where `ModalDialog`'s inherited `overflow-wrap: anywhere`
 * broke it one character per line — a tall vertical stack of letters, in a
 * shipped popup, on a real 0.34.0 entry.
 *
 * The measurement is geometry, because that's where the bug lives: nothing about
 * the DOM was wrong, and jsdom computes no layout. Every inline element inside an
 * entry has to be wider than it is tall, which no character stack can be.
 *
 * It runs off the dialog gallery's `several-releases` fixture (committed copy with
 * `code`, `strong`, and a `code` span inside the lead's numbered list) rather than
 * the real changelog slice, so the coverage can't evaporate the day the last five
 * releases happen to ship plain-prose entries only.
 *
 * ❗ Needs a gallery-carrying binary, same as `dialog-inset.spec.ts`:
 * `CMDR_E2E_BUILD=1` (set by `test:e2e:playwright:build`) turns on the
 * `__CMDR_DIALOG_GALLERY__` define. Locally it SKIPS with the recipe; under CI it
 * FAILS, because a silent skip there reads as coverage that isn't happening.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, dismissOverlay } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** Opens one gallery state exactly the way Debug > Soft dialogs does. */
function openGalleryState(page: TauriPage, dialogId: string, stateId: string): Promise<void> {
  const payload = JSON.stringify({ dialogId, stateId, fixtures: null })
  return page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'debug-open-gallery-dialog',
        payload: ${payload}
    })`)
}

/** One measured inline element: what it is, what it says, and its box. */
interface MarkupBox {
  tag: string
  text: string
  width: number
  height: number
}

/**
 * Every inline element inside an entry or a lead list item, with its box. Read
 * after the details disclosure is open, so the Fixed entries are laid out.
 */
function measureInlineMarkup(page: TauriPage): Promise<MarkupBox[]> {
  return page.evaluate<MarkupBox[]>(`
    (function () {
      const items = document.querySelectorAll('#whats-new-body .entries li, #whats-new-body .lead li')
      const boxes = []
      for (const item of items) {
        for (const el of item.querySelectorAll('code, strong, em, a')) {
          const rect = el.getBoundingClientRect()
          boxes.push({
            tag: el.tagName.toLowerCase(),
            text: (el.textContent || '').trim(),
            width: rect.width,
            height: rect.height
          })
        }
      }
      return boxes
    })()
  `)
}

/** Opens every release's "Show more", so the entry lists are laid out and measurable. */
function expandEveryRelease(page: TauriPage): Promise<void> {
  return page.evaluate(`
    (function () {
      const toggles = document.querySelectorAll('#whats-new-body .details-toggle')
      for (const toggle of toggles) {
        if (toggle.getAttribute('aria-expanded') !== 'true') toggle.click()
      }
    })()
  `)
}

test.describe("What's new inline markup", () => {
  test.describe.configure({ timeout: 60000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  // In the hook, never only at the end of the test: a failed measurement throws, and the
  // popup left standing would trip the leak guard on top of the real failure.
  test.afterEach(async ({ tauriPage }) => {
    await dismissOverlay(tauriPage).catch(() => {})
  })

  test('a code span or a bold run stays on the entry’s own line', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage

    await openGalleryState(page, 'whats-new', 'several-releases')
    const galleryLive = await page
      .waitForSelector('#whats-new-body', 4000)
      .then(() => true)
      .catch(() => false)
    const noGallery =
      'This binary carries no dialog gallery (`CMDR_E2E_BUILD=1` sets the ' +
      '`__CMDR_DIALOG_GALLERY__` define). Rebuild with `pnpm test:e2e:playwright:build`.'
    if (!galleryLive && process.env.CI) throw new Error(noGallery)
    test.skip(!galleryLive, noGallery)

    await expandEveryRelease(page)
    // The disclosure animates a 0fr → 1fr grid row, so the entries reach their real
    // size a frame or two after the click.
    await expect.poll(async () => (await measureInlineMarkup(page)).length, { timeout: 3000 }).toBeGreaterThan(0)
    const boxes = await measureInlineMarkup(page)

    // The fixture's own count: two `code` spans and one `strong` among the Fixed
    // entries, one `code` in the lead's numbered list, plus the leads' bold
    // headlines. Fewer means the fixture lost its inline copy and this test is
    // measuring nothing.
    expect(boxes.length, `measured markup: ${JSON.stringify(boxes)}`).toBeGreaterThanOrEqual(4)

    const stacked = boxes.filter((box) => box.width <= box.height)
    expect(
      stacked,
      `inline markup wrapped into the bullet column and stacked one character per line: ${JSON.stringify(stacked)}`,
    ).toEqual([])

    await dismissOverlay(page)
    await expect.poll(() => page.isVisible('#whats-new-body').catch(() => false), { timeout: 3000 }).toBeFalsy()
  })
})
