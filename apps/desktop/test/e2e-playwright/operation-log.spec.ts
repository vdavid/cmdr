/**
 * E2E for the alpha "Operation log" dialog (M7): it opens BOTH from the View menu
 * item and from the ⌘⌥L keyboard shortcut, and renders with the ALPHA badge.
 *
 * The menu path uses `dispatchMenuCommand` (menu click → on_menu_event →
 * execute-command, exactly what the native menu emits). The shortcut path
 * dispatches a real keydown so it exercises `handleGlobalKeyDown` →
 * `lookupCommand('⌘⌥L')` → the `log.operationLog` handler — the same JS dispatch a
 * key press hits. The instance's journal may be empty, so the assertion is that
 * the dialog and its ALPHA badge render (grouped-row rendering + paging are
 * covered by the component/unit tests), not that any specific operation shows.
 */

import { test, expect } from './fixtures.js'
import { dispatchMenuCommand, dismissOverlay, ensureAppReady, CTRL_OR_META } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** The dialog is open once its body element is in the DOM. */
function dialogOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('#operation-log-body') !== null`)
}

/** The ALPHA stability badge is present in the open dialog's title. */
function alphaBadgeShown(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(
    `(document.querySelector('.feature-status-badge')?.textContent || '').trim().toLowerCase() === 'alpha'`,
  )
}

/** Dispatches the default ⌘⌥L keydown (Ctrl+Alt+L on Linux), matching the
 *  platform-format shortcut string the dispatch map holds. */
function pressOperationLogShortcut(page: TauriPage): Promise<void> {
  return page.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'l', code: 'KeyL',
        ctrlKey: ${String(CTRL_OR_META === 'Control')},
        metaKey: ${String(CTRL_OR_META === 'Meta')},
        altKey: true, bubbles: true
    }))`)
}

test.describe('Operation log dialog', () => {
  test.describe.configure({ timeout: 30000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  test('opens from the View menu item with the ALPHA badge', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await dispatchMenuCommand(page, 'log.operationLog')

    await expect.poll(() => dialogOpen(page), { timeout: 3000 }).toBe(true)
    expect(await alphaBadgeShown(page)).toBe(true)

    await dismissOverlay(page)
    await expect.poll(() => dialogOpen(page), { timeout: 3000 }).toBe(false)
  })

  test('opens from the ⌘⌥L keyboard shortcut with the ALPHA badge', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    // Re-press inside the poll: the menu test above fired `log.operationLog` from
    // the `menu` source, and the cross-source double-fire guard (dispatch-dedup.ts,
    // 300ms window) drops a `keyboard` fire of the same command inside that window.
    // A dropped fire doesn't refresh the window, so pressing again once it lapses
    // opens the dialog — an active probe, not a fixed wait. `openOperationLog()` is
    // idempotent, so extra presses after it opens are harmless.
    await expect
      .poll(
        async () => {
          await pressOperationLogShortcut(page)
          return dialogOpen(page)
        },
        { timeout: 5000 },
      )
      .toBe(true)
    expect(await alphaBadgeShown(page)).toBe(true)

    await dismissOverlay(page)
    await expect.poll(() => dialogOpen(page), { timeout: 3000 }).toBe(false)
  })
})
