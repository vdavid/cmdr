/**
 * Driving the Ask Cmdr rail from a spec: opening it past its gates, and telling when a turn is over.
 * `ask-cmdr.spec.ts`, `ask-cmdr-capability.spec.ts`, and `about-this-verdict.spec.ts` share these.
 */

import { waitBudget } from './wait-budget.js'
import { expect } from './fixtures.js'
import {
  closeScopedWindow,
  dispatchMenuCommand,
  openSettingsWindowViaProd,
  waitForAskCmdrOnInBackend,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** The rail is open once its root element is in the DOM. */
export function railOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail') !== null`)
}

/** Opens the rail via the View-menu toggle, re-dispatching inside the poll: the
 * cross-source double-fire guard (dispatch-dedup.ts, 300ms) can drop a menu fire that
 * lands right after a prior test's toggle. Idempotent once open. */
export async function openRailViaMenu(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await railOpen(page)) return true
        await dispatchMenuCommand(page, 'askCmdr.toggle')
        return railOpen(page)
      },
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
}

/** Closes the rail via its header close button, if open, so state doesn't leak between tests. */
export async function closeRailIfOpen(page: TauriPage): Promise<void> {
  if (!(await railOpen(page))) return
  await page.evaluate(`document.querySelector('.ask-cmdr-rail .header-actions button:last-child')?.click()`)
  await expect.poll(() => railOpen(page), { timeout: waitBudget(3000) }).toBe(false)
}

/** The rail's "Ask Cmdr is off" gate is showing (the rail is open, the chat isn't). */
export function offGateShown(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail .ask-cmdr-gate[data-gate="off"]') !== null`)
}

/** The composer is present (the rail is past its gates). */
export function composerPresent(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail textarea') !== null`)
}

/** Get the rail ready for a chat test: turn Ask Cmdr on from the rail's gate if it's showing
 * (the switch persists in `settings.json` for the run, so it's a no-op once on), then wait for
 * the composer to render. The gate resolves asynchronously on open, so the composer isn't
 * present on the first tick even when already on — always wait. The E2E fake keeps
 * `ai.provider` off, so the cloud gate never shows here. */
export async function ensureChatReady(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await composerPresent(page)) return true
        if (await offGateShown(page)) {
          await page.evaluate(
            `document.querySelector('.ask-cmdr-rail .ask-cmdr-gate[data-gate="off"] button')?.click()`,
          )
        }
        return composerPresent(page)
      },
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
  // The rail flips at once; the backend learns the switch a save and a push later.
  await waitForAskCmdrOnInBackend(page)
}

/** The rail's last assistant bubble, as an expression. */
export const LAST_BUBBLE_JS = `[...document.querySelectorAll('.ask-cmdr-rail .msg')].filter(m => m.querySelector('.tools, .prose')).at(-1)`

/** The last bubble's turn has ended: its stats line shows and nothing streams. */
export function lastTurnSettled(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`(() => {
    const bubble = ${LAST_BUBBLE_JS};
    return !!bubble && !!bubble.querySelector('.turn-stats') && !bubble.querySelector('.cursor');
  })()`)
}

/** Types `text` into the composer and presses Enter. */
export async function typeAndSend(page: TauriPage, text: string): Promise<void> {
  await page.evaluate(`(() => {
    const ta = document.querySelector('.ask-cmdr-rail textarea');
    ta.focus();
    ta.value = ${JSON.stringify(text)};
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  })()`)
}

/** The Ask Cmdr switch in Settings > AI > Ask Cmdr: its hidden input carries the label. */
const ASK_CMDR_SWITCH = 'input[aria-label="Ask Cmdr"]'

/** The switch's state as Ark draws it (`checked` / `unchecked`), or `missing`. */
function askCmdrSwitchStateJs(): string {
  return `(function() {
    var input = document.querySelector('${ASK_CMDR_SWITCH}');
    if (!input) return 'missing';
    var root = input.closest('[data-scope="switch"][data-part="root"]');
    var control = root ? root.querySelector('.switch-control') : null;
    return (control || input).getAttribute('data-state') || 'unknown';
  })()`
}

/** Sets Ask Cmdr's switch through the Settings window, the way a user does, and closes it. */
export async function setAskCmdrEnabledInSettings(main: TauriPage, on: boolean): Promise<void> {
  const settings = await openSettingsWindowViaProd(main)
  try {
    await settings.waitForSelector('.settings-sidebar', waitBudget(3000))
    await settings.evaluate(`(function() {
      var items = document.querySelectorAll('.section-item');
      for (var i = 0; i < items.length; i++) {
        if ((items[i].textContent || '').trim() === 'Ask Cmdr') { items[i].click(); return; }
      }
    })()`)
    await settings.waitForSelector(ASK_CMDR_SWITCH, waitBudget(3000))
    const wanted = on ? 'checked' : 'unchecked'
    if ((await settings.evaluate<string>(askCmdrSwitchStateJs())) !== wanted) {
      await settings.evaluate(`document.querySelector('${ASK_CMDR_SWITCH}')?.click()`)
    }
    await expect
      .poll(() => settings.evaluate<string>(askCmdrSwitchStateJs()), { timeout: waitBudget(3000) })
      .toBe(wanted)
  } finally {
    await closeScopedWindow(main, settings, 'settings')
  }
}
