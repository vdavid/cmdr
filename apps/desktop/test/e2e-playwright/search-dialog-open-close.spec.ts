/**
 * Search dialog: open + close lifecycle.
 *
 * The smallest possible smoke for the dialog. Confirms that:
 *   1. The registry's `search.open` command (the same one the menu and `⌘F`
 *      menu accelerator wire) mounts `.search-overlay`.
 *   2. Escape unmounts it.
 *
 * Anything richer (mode chips, results, filters) is covered by sibling specs.
 * This one is intentionally cheap (~600 ms wall-clock) so it acts as the
 * canary if either the registry wiring or the dialog's Escape handler
 * regresses.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { SEARCH_OVERLAY, closeSearchDialog, openSearchDialog, resetSearchDialog } from './search-helpers.js'

test.describe('Search dialog: open and close', () => {
  test('search.open mounts the overlay, Escape closes it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await openSearchDialog(tauriPage)
    expect(await tauriPage.count(SEARCH_OVERLAY)).toBe(1)

    await closeSearchDialog(tauriPage)
    expect(await tauriPage.count(SEARCH_OVERLAY)).toBe(0)

    // Reopening after close should work uneventfully (state preservation lives
    // in `search-state.svelte.ts`, but the overlay element itself is fresh).
    await openSearchDialog(tauriPage)
    const reopened = await pollUntil(
      tauriPage,
      async () => (await tauriPage.count(SEARCH_OVERLAY)) === 1,
      waitBudget(2000),
    )
    expect(reopened).toBe(true)
    await closeSearchDialog(tauriPage)
  })

  // Off macOS every key the dialog shows is that OS's own spelling (`query-keys.ts` and the
  // `search.*` fixed commands), so no Mac glyph may reach the screen.
  test('names its keys in the OS’s own spelling', async ({ tauriPage }) => {
    test.skip(process.platform === 'darwin', 'the Mac glyphs are the macOS spelling')
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await openSearchDialog(tauriPage)
    await resetSearchDialog(tauriPage)
    const text = await tauriPage.evaluate<string>(
      `document.querySelector(${JSON.stringify(SEARCH_OVERLAY)})?.textContent ?? ''`,
    )
    expect(text).toContain('Alt+F')
    expect(text).toContain('Ctrl+N')
    expect(text).not.toMatch(/[⌘⌥⌃⇧⏎]/)
    await closeSearchDialog(tauriPage)
  })
})
