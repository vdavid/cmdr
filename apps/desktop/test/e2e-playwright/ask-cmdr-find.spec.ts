/**
 * E2E for the Ask Cmdr rail's in-chat find: ⌘F with focus in the rail opens the find bar
 * (and Search files doesn't open), while ⌘F with focus in a pane still opens Search files;
 * typing counts the hits; Enter / ⇧Enter and ⌘G / ⇧⌘G step and wrap; Escape closes the bar
 * and hands focus back to the prompt box.
 *
 * The thread is seeded through the scripted fake LLM (`CMDR_E2E_ASK_CMDR_FAKE=1`): two sends
 * carrying a per-run nonce, so the find has exactly two user-bubble hits, plus the fake's two
 * fixed replies. Keys are real keydowns on the focused element, so they travel the rail's
 * listener and the document dispatcher in the order the app's own keys do.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { closeRailIfOpen, ensureChatReady, openRailViaMenu, typeAndSend } from './ask-cmdr-helpers.js'
import { ensureAppReady, CTRL_OR_META } from './helpers.js'
import { SEARCH_OVERLAY, closeSearchDialog } from './search-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const FIND_BAR = '.ask-cmdr-rail [role="search"]'
const FIND_FIELD = `${FIND_BAR} input`

/** Presses `key` on whatever has focus, with the platform's ⌘ when `mod` is set. */
function pressOnFocused(page: TauriPage, key: string, opts: { mod?: boolean; shift?: boolean } = {}): Promise<void> {
  const meta = opts.mod === true && CTRL_OR_META === 'Meta'
  const ctrl = opts.mod === true && CTRL_OR_META === 'Control'
  return page.evaluate(`(document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', {
    key: ${JSON.stringify(key)}, metaKey: ${String(meta)}, ctrlKey: ${String(ctrl)},
    shiftKey: ${String(opts.shift === true)}, bubbles: true, cancelable: true
  }))`)
}

function findBarShown(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector(${JSON.stringify(FIND_BAR)}) !== null`)
}

function counter(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `(document.querySelector(${JSON.stringify(`${FIND_BAR} .match-count`)})?.textContent || '').trim()`,
  )
}

function focusIs(page: TauriPage, selector: string): Promise<boolean> {
  return page.evaluate<boolean>(`document.activeElement === document.querySelector(${JSON.stringify(selector)})`)
}

/** How many fake replies the open thread shows. */
function replyCount(page: TauriPage): Promise<number> {
  return page.evaluate<number>(
    `[...document.querySelectorAll('.ask-cmdr-rail .msg .prose')].filter(p => (p.textContent || '').includes('test assistant')).length`,
  )
}

/** A fresh chat with two sends naming `nonce`, both answered. */
async function seedThread(page: TauriPage, nonce: string): Promise<void> {
  await page.evaluate(`document.querySelector('.ask-cmdr-rail [data-rail-action="new-chat"]')?.click()`)
  await expect.poll(() => replyCount(page), { timeout: waitBudget(3000) }).toBe(0)
  await typeAndSend(page, `first ${nonce} question`)
  await expect.poll(() => replyCount(page), { timeout: waitBudget(8000) }).toBe(1)
  await typeAndSend(page, `second ${nonce} question`)
  await expect.poll(() => replyCount(page), { timeout: waitBudget(8000) }).toBe(2)
}

async function typeQuery(page: TauriPage, query: string): Promise<void> {
  await page.evaluate(`(() => {
    const input = document.querySelector(${JSON.stringify(FIND_FIELD)});
    input.value = ${JSON.stringify(query)};
    input.dispatchEvent(new Event('input', { bubbles: true }));
  })()`)
}

test.describe('Ask Cmdr in-chat find', () => {
  test.describe.configure({ timeout: waitBudget(45000) })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    if ((await page.count(SEARCH_OVERLAY)) > 0) await closeSearchDialog(page)
    await closeRailIfOpen(page)
  })

  test('⌘F finds in the chat from the rail, and still opens Search files from a pane', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await openRailViaMenu(page)
    await ensureChatReady(page)
    const nonce = `findnonce${String(Date.now())}`
    await seedThread(page, nonce)

    // Focus in the rail (the prompt box): ⌘F opens the find bar, not Search files.
    await page.evaluate(`document.querySelector('.ask-cmdr-rail textarea')?.focus()`)
    await pressOnFocused(page, 'f', { mod: true })
    await expect.poll(() => findBarShown(page), { timeout: waitBudget(3000) }).toBe(true)
    await expect.poll(() => focusIs(page, FIND_FIELD), { timeout: waitBudget(3000) }).toBe(true)
    expect(await page.count(SEARCH_OVERLAY)).toBe(0)

    // Typing counts the hits: the two user bubbles carry the nonce.
    await typeQuery(page, nonce)
    await expect.poll(() => counter(page), { timeout: waitBudget(3000) }).toBe('1 of 2')

    // Enter / ⇧Enter in the field step and wrap.
    await pressOnFocused(page, 'Enter')
    await expect.poll(() => counter(page), { timeout: waitBudget(2000) }).toBe('2 of 2')
    await pressOnFocused(page, 'Enter')
    await expect.poll(() => counter(page), { timeout: waitBudget(2000) }).toBe('1 of 2')
    await pressOnFocused(page, 'Enter', { shift: true })
    await expect.poll(() => counter(page), { timeout: waitBudget(2000) }).toBe('2 of 2')

    // ⌘G / ⇧⌘G step from elsewhere in the rail too, and Go to path stays shut.
    await page.evaluate(`document.querySelector('.ask-cmdr-rail textarea')?.focus()`)
    await pressOnFocused(page, 'g', { mod: true })
    await expect.poll(() => counter(page), { timeout: waitBudget(2000) }).toBe('1 of 2')
    await pressOnFocused(page, 'G', { mod: true, shift: true })
    await expect.poll(() => counter(page), { timeout: waitBudget(2000) }).toBe('2 of 2')
    expect(await page.count('[data-dialog-id="go-to-path"]')).toBe(0)

    // The fake's replies are found too: the assistant's prose is searched.
    await pressOnFocused(page, 'f', { mod: true })
    await expect.poll(() => focusIs(page, FIND_FIELD), { timeout: waitBudget(3000) }).toBe(true)
    await typeQuery(page, 'test assistant')
    await expect.poll(() => counter(page), { timeout: waitBudget(3000) }).toMatch(/^\d of 2$/)

    // Escape in the field closes the bar and hands focus back to the prompt box.
    await pressOnFocused(page, 'Escape')
    await expect.poll(() => findBarShown(page), { timeout: waitBudget(2000) }).toBe(false)
    await expect.poll(() => focusIs(page, '.ask-cmdr-rail textarea'), { timeout: waitBudget(2000) }).toBe(true)

    // Focus in a pane: ⌘F is Search files, and the rail's find stays shut.
    await page.evaluate(`document.querySelector('.file-pane')?.click()`)
    await expect
      .poll(
        () => page.evaluate<boolean>(`document.querySelector('.file-pane')?.classList.contains('is-focused') === true`),
        {
          timeout: waitBudget(3000),
        },
      )
      .toBe(true)
    await expect
      .poll(
        () =>
          page.evaluate<boolean>(
            `document.querySelector('.dual-pane-explorer')?.contains(document.activeElement) === true`,
          ),
        { timeout: waitBudget(3000) },
      )
      .toBe(true)
    await pressOnFocused(page, 'f', { mod: true })
    await expect.poll(() => page.count(SEARCH_OVERLAY), { timeout: waitBudget(3000) }).toBe(1)
    expect(await findBarShown(page)).toBe(false)
    await closeSearchDialog(page)
  })
})
