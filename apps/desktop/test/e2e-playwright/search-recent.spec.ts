/**
 * Search dialog: recent searches persistence.
 *
 * Per plan §3.5, only "Open in pane" adds to the history (auto-applies and
 * Enter-runs don't). The dialog's `openInPane` handler calls `addRecentSearch`
 * as a fire-and-forget IPC, then closes the dialog. The footer's in-memory
 * cache is loaded once per session (`loadRecentSearches` idempotent), so the
 * fresh entry shows up next session — or, in the same session, only after a
 * forced refetch via the `getRecentSearches` IPC.
 *
 * This test exercises the persistence half (the contract that "Open in pane
 * adds to the backend"): after clicking the button, we force-refetch via
 * `getRecentSearches` IPC and confirm the seeded query lands in the returned
 * list. The render-side half (the chip lights up automatically) is the
 * cross-session behavior pinned by `recent-searches-state.svelte.ts`'s own
 * Vitest contracts; doing it again here would re-test the cache rather than
 * the persistence.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil } from './helpers.js'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'

// The footer button is labelled "Show all in main window" and is always in the DOM,
// disabled until results land. `:not([disabled])` matters — without it, the selector
// matches the disabled state and the test clicks a no-op.
const OPEN_IN_PANE_BUTTON = '.search-overlay [aria-label="Show all in main window"]:not([disabled])'

test.describe('Search dialog: recent searches', () => {
  test('Open-in-pane persists the query to the backend recent-search store', async ({ tauriPage }) => {
    // 15 s: opens the dialog through MCP (which roundtrips through HTTP),
    // waits for the index to land a result, then polls the persistence
    // IPC. The 8 s default is too tight for the combined latency.
    test.setTimeout(15000)
    // Defensive `.search-overlay` cleanup. The global afterEach safety net in
    // fixtures.ts auto-cleans leaked overlays after each test, BUT this spec's
    // beforeEach drives the search dialog into a specific prefill state via
    // MCP (`open_search_dialog` with `autoRun: true`); reopening from a
    // stale-but-just-auto-cleaned state can race the prefill listener and
    // leave the dialog without results. Re-dismiss here for determinism.
    await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector('.search-overlay');
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    })()`)
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    // Open the dialog via the MCP `open_search_dialog` tool, prefilling a
    // Filename-mode query and asking for autoRun. This bypasses the dialog's
    // preserved-state pitfall (where prior tests leave the query / mode dirty)
    // and the AI-mode default that would otherwise need a live provider to land
    // results.
    const seededQuery = 'file'
    await mcpCall('open_search_dialog', { query: seededQuery, mode: 'filename', autoRun: true })

    // The footer's "Open in pane" only renders once `resultCount > 0`. Waiting
    // for the button is the observable signal that the search ran and landed
    // results.
    await tauriPage.waitForSelector(OPEN_IN_PANE_BUTTON, 5000)

    await tauriPage.click(OPEN_IN_PANE_BUTTON)

    // Poll the backend's `get_recent_searches` IPC directly. The `addRecentSearch`
    // call inside `openInPane` is fire-and-forget; we poll for the entry to
    // land in persistent storage. Bypasses the dialog's in-memory cache
    // (idempotent per session) so the assertion doesn't depend on the
    // cross-session render path.
    const found = await pollUntil(
      tauriPage,
      async () => {
        const queries = await tauriPage.evaluate<string[]>(`(async function(){
            var invoke = window.__TAURI_INTERNALS__.invoke;
            var entries = await invoke('get_recent_searches', { limit: null });
            return entries.map(function(e) { return e.query; });
        })()`)
        return queries.includes(seededQuery)
      },
      3000,
    )
    expect(found).toBe(true)
  })
})
