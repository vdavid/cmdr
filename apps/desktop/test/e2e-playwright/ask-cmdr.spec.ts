/**
 * E2E for the alpha "Ask Cmdr" chat rail: it toggles open from BOTH the View menu item and
 * the ⌘⌥A keyboard shortcut (with the ALPHA badge), focuses the composer on open and
 * returns focus to the panes on Escape, and streams a reply end-to-end.
 *
 * The send path runs against the deterministic scripted fake LLM (the app launches with
 * `CMDR_E2E_ASK_CMDR_FAKE=1`; see the e2e-playwright check + commands/agent.rs), so the
 * reply text is fixed and no provider is needed. The menu path uses `dispatchMenuCommand`
 * (native menu → execute-command); the shortcut path dispatches a real keydown through
 * `handleGlobalKeyDown` → `lookupCommand('⌘⌥A')` → the `askCmdr.toggle` handler.
 */

import { test, expect } from './fixtures.js'
import { dispatchMenuCommand, ensureAppReady, CTRL_OR_META } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** The rail is open once its root element is in the DOM. */
function railOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail') !== null`)
}

/** The ALPHA stability badge is present in the open rail's header. */
function alphaBadgeShown(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(
    `(document.querySelector('.ask-cmdr-rail .feature-status-badge')?.textContent || '').trim().toLowerCase() === 'alpha'`,
  )
}

/** True when focus is on the rail's composer textarea. */
function composerFocused(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.activeElement === document.querySelector('.ask-cmdr-rail textarea')`)
}

/** True when focus is back inside the dual-pane explorer. */
function paneFocused(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(
    `document.querySelector('.dual-pane-explorer')?.contains(document.activeElement) === true`,
  )
}

/** Dispatches the default ⌘⌥A keydown (Ctrl+Alt+A on Linux). */
function pressToggleShortcut(page: TauriPage): Promise<void> {
  return page.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'a', code: 'KeyA',
        ctrlKey: ${String(CTRL_OR_META === 'Control')},
        metaKey: ${String(CTRL_OR_META === 'Meta')},
        altKey: true, bubbles: true
    }))`)
}

/** Closes the rail via its header close button, if open, so state doesn't leak between tests. */
async function closeRailIfOpen(page: TauriPage): Promise<void> {
  if (!(await railOpen(page))) return
  await page.evaluate(`document.querySelector('.ask-cmdr-rail .header-actions button:last-child')?.click()`)
  await expect.poll(() => railOpen(page), { timeout: 3000 }).toBe(false)
}

test.describe('Ask Cmdr rail', () => {
  test.describe.configure({ timeout: 30000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeRailIfOpen(tauriPage as TauriPage)
  })

  test('opens from the View menu item with the ALPHA badge', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await dispatchMenuCommand(page, 'askCmdr.toggle')
    await expect.poll(() => railOpen(page), { timeout: 3000 }).toBe(true)
    expect(await alphaBadgeShown(page)).toBe(true)
  })

  test('opens from the ⌘⌥A keyboard shortcut', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    // Re-press inside the poll: the cross-source double-fire guard (dispatch-dedup.ts,
    // 300ms) can drop a keyboard fire right after a menu fire from a prior test. The
    // toggle is idempotent while open, so extra presses after it opens are harmless.
    await expect
      .poll(
        async () => {
          if (await railOpen(page)) return true
          await pressToggleShortcut(page)
          return railOpen(page)
        },
        { timeout: 5000 },
      )
      .toBe(true)
  })

  test('focuses the composer on open and returns focus to the panes on Escape', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await dispatchMenuCommand(page, 'askCmdr.toggle')
    await expect.poll(() => railOpen(page), { timeout: 3000 }).toBe(true)
    await expect.poll(() => composerFocused(page), { timeout: 3000 }).toBe(true)

    // Escape on the focused composer returns focus to the active pane.
    await page.evaluate(
      `document.querySelector('.ask-cmdr-rail textarea')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`,
    )
    await expect.poll(() => paneFocused(page), { timeout: 3000 }).toBe(true)
  })

  test('sends a message and streams the fake reply', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await dispatchMenuCommand(page, 'askCmdr.toggle')
    await expect.poll(() => railOpen(page), { timeout: 3000 }).toBe(true)

    // Type into the composer and press Enter (Svelte's bind:value listens for input).
    await page.evaluate(`(() => {
      const ta = document.querySelector('.ask-cmdr-rail textarea');
      ta.focus();
      ta.value = 'hello there';
      ta.dispatchEvent(new Event('input', { bubbles: true }));
      ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`)

    // The scripted fake streams "Hi! I'm the test assistant." into the assistant bubble.
    await expect
      .poll(
        () =>
          page.evaluate<boolean>(
            `(document.querySelector('.ask-cmdr-rail')?.textContent || '').includes('test assistant')`,
          ),
        { timeout: 8000 },
      )
      .toBe(true)
  })
})
