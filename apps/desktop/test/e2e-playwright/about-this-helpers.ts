/**
 * Shared by the "Tell me about this" specs.
 *
 * The footer is Rust's (`cmdr_about::actions`): the first three actions show, the rest sit behind "More", and which
 * ones show depends on state another spec may have left (Ask Cmdr on adds "Continue in Ask Cmdr" before the removal
 * action, pushing Move to trash behind "More"). So a spec presses an action wherever the footer put it, ❌ never
 * assuming it's shown.
 */

import { waitBudget } from './wait-budget.js'
import { expect } from './fixtures.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

export const ABOUT_CARD = '[data-dialog-id="tell-me-about-this"]'

/**
 * Presses the first of `actions` (`AboutAction` type tokens) the card's footer offers, shown or behind "More". A shown
 * one is pressed from the keyboard's side: focused (it's in the Tab order), then activated with its own `click()`,
 * which is what a real Enter on a focused button fires. One behind "More" is chosen from the opened menu's row.
 */
export async function pressFooterAction(page: PageLike, actions: readonly string[]): Promise<void> {
  const shownSelector = `${ABOUT_CARD} :is(${actions.map((a) => `[data-action="${a}"]`).join(', ')}) button`
  const pressedShown = await page.evaluate<boolean>(`(() => {
    const button = document.querySelector(${JSON.stringify(shownSelector)})
    if (!button) return false
    button.focus()
    if (document.activeElement !== button) return false
    button.click()
    return true
  })()`)
  if (pressedShown) return

  const rowSelector = `[data-menu] :is(${actions.map((a) => `[data-menu-row="${a}"]`).join(', ')})`
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(`(() => {
          const row = document.querySelector(${JSON.stringify(rowSelector)})
          if (row) { row.click(); return true }
          // Open "More" once; a second click would close it again.
          if (!document.querySelector('[data-menu]')) {
            document.querySelector(${JSON.stringify(`${ABOUT_CARD} [data-action="more"] button`)})?.click()
          }
          return false
        })()`),
      { timeout: waitBudget(3000) },
    )
    .toBe(true)
}
