/**
 * E2E for searching past chats in the Ask Cmdr threads panel: real FTS5 over persisted
 * messages, ranked by the backend, each result showing its age and a marked excerpt, and the
 * keyboard path the panel promises (↓ from the field, Enter opens).
 *
 * Threads are seeded by sending through the deterministic fake LLM (`CMDR_E2E_ASK_CMDR_FAKE=1`),
 * each carrying a per-run nonce so threads left by earlier runs or specs never match.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { closeRailIfOpen, ensureChatReady, openRailViaMenu, typeAndSend } from './ask-cmdr-helpers.js'
import { ensureAppReady } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const SEARCH_FIELD = `.ask-cmdr-rail .sessions input.text-field-control`

function fakeReplyCount(page: TauriPage): Promise<number> {
  return page.evaluate<number>(
    `[...document.querySelectorAll('.ask-cmdr-rail .msg')].filter(m => (m.textContent||'').includes('test assistant')).length`,
  )
}

/** Starts a fresh chat and sends `text` into it, waiting for the fake's reply. */
async function seedThread(page: TauriPage, text: string): Promise<void> {
  await page.evaluate(`document.querySelector('.ask-cmdr-rail [aria-label="New chat"]')?.click()`)
  const before = await fakeReplyCount(page)
  await typeAndSend(page, text)
  await expect.poll(() => fakeReplyCount(page), { timeout: waitBudget(8000) }).toBeGreaterThan(before)
}

interface ResultRow {
  title: string
  age: string
  marked: string[]
  selected: boolean
}

function resultRows(page: TauriPage): Promise<ResultRow[]> {
  return page.evaluate<
    ResultRow[]
  >(`[...document.querySelectorAll('.ask-cmdr-rail .sessions [role="option"]')].map(o => ({
    title: o.querySelector('.row-title')?.textContent ?? '',
    age: o.querySelector('.hit-age')?.textContent ?? '',
    marked: [...o.querySelectorAll('mark')].map(m => m.textContent),
    selected: o.getAttribute('aria-selected') === 'true',
  }))`)
}

test.describe('Ask Cmdr chat search', () => {
  test.describe.configure({ timeout: waitBudget(45000) })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test('ranks matches, shows age and a marked excerpt, and opens one from the keyboard', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const nonce = `n${String(Date.now())}`
    // Under 60 characters each, so the thread's title is the whole message.
    const passing = `${nonce} garden plans for autumn, maybe a kumquat tree`
    const focused = `${nonce} kumquat jam from kumquat`
    const unrelated = `${nonce} grocery list for the week`

    await openRailViaMenu(page)
    await ensureChatReady(page)
    // The focused thread goes in FIRST, so ranking it on top takes relevance, not recency.
    await seedThread(page, focused)
    await seedThread(page, passing)
    await seedThread(page, unrelated)

    await page.evaluate(`document.querySelector('.ask-cmdr-rail [aria-label="Chats"]')?.click()`)
    await expect
      .poll(() => page.evaluate<boolean>(`document.querySelector(${JSON.stringify(SEARCH_FIELD)}) !== null`), {
        timeout: waitBudget(3000),
      })
      .toBe(true)
    await page.evaluate(`(() => {
      const input = document.querySelector(${JSON.stringify(SEARCH_FIELD)});
      input.focus();
      input.value = ${JSON.stringify(`${nonce} kumquat`)};
      input.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)

    // Two matches, the older one ABOUT kumquats first; the grocery list doesn't match.
    await expect.poll(async () => (await resultRows(page)).length, { timeout: waitBudget(5000) }).toBe(2)
    const rows = await resultRows(page)
    expect(rows.map((row) => row.title)).toEqual([focused, passing])
    expect(rows[0].marked).toEqual([nonce, 'kumquat', 'kumquat'])
    expect(rows[0].selected).toBe(true)
    for (const row of rows) expect(row.age).not.toBe('')

    // ↓ from the field moves the cursor; focus stays in the field.
    await page.keyboard.press('ArrowDown')
    await expect
      .poll(async () => (await resultRows(page)).map((row) => row.selected), { timeout: waitBudget(3000) })
      .toEqual([false, true])
    expect(
      await page.evaluate<boolean>(
        `document.activeElement === document.querySelector(${JSON.stringify(SEARCH_FIELD)})`,
      ),
    ).toBe(true)

    // Enter opens the cursor's thread and closes the panel.
    await page.keyboard.press('Enter')
    await expect
      .poll(() => page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail .sessions') === null`), {
        timeout: waitBudget(3000),
      })
      .toBe(true)
    await expect
      .poll(
        () =>
          page.evaluate<boolean>(
            `(document.querySelector('.ask-cmdr-rail')?.textContent || '').includes(${JSON.stringify(passing)})`,
          ),
        { timeout: waitBudget(3000) },
      )
      .toBe(true)
  })
})
