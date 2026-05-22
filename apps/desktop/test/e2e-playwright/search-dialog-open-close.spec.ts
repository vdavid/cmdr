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

import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { SEARCH_OVERLAY, closeSearchDialog, openSearchDialog } from './search-helpers.js'

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
    const reopened = await pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 1, 2000)
    expect(reopened).toBe(true)
    await closeSearchDialog(tauriPage)
  })
})
