/**
 * Search dialog: recent searches persistence.
 *
 * Per plan §3.5, only "Open in pane" adds to the history (auto-applies and
 * Enter-runs don't). The dialog's `openInPane` handler calls `addRecentSearch`
 * as a fire-and-forget IPC, then closes the dialog. The query field's
 * recent-items dropdown reads an in-memory cache loaded once per session
 * (`loadRecentSearches` is idempotent), so the fresh entry shows up next
 * session — or, in the same session, only after a forced refetch via the
 * `getRecentSearches` IPC.
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
import { ensureAppReady, getFixtureRoot, pollUntil } from './helpers.js'
import { ensureLocalIndexAnswers } from './search-walk-ground.js'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'

// The footer action button is labelled "Show all in main window" and is always in the DOM,
// disabled until results land. `:not([disabled])` matters — without it, the selector
// matches the disabled state and the test clicks a no-op.
const OPEN_IN_PANE_BUTTON = '.search-overlay [aria-label="Show all in main window"]:not([disabled])'

test.describe('Search dialog: recent searches', () => {
  test('Open-in-pane persists the query to the backend recent-search store', async ({ tauriPage }) => {
    // 45 s of HEADROOM, not a wait anybody expects to spend: an index-served run lands
    // results in well under a second, so the body finishes in ~1.3 s and the cap is
    // never approached. It covers only the rare branch where `ensureLocalIndexAnswers`
    // has to REBUILD an index a previous spec left empty, which waits on a full rescan
    // (30 s) plus a probe (20 s) and cannot fit in a tight budget. ❗ A 15 s cap turned
    // that branch into a guaranteed red instead of a slow pass.
    test.setTimeout(45000)
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

    // ❗ Prove the index can answer for this scope BEFORE opening the dialog. The
    // live-walk specs earlier in this shard (`search-live`, `search-walk-handoff`)
    // empty the local index and put it back from an `afterAll`; a restore that didn't
    // complete leaves the auto-run below with nothing to read, and if another spec's
    // walk still claims the ground the run parks with no deadline at all. An
    // index-served run can't park, and it lands results in ~0.3 s.
    await ensureLocalIndexAnswers()

    // Open the dialog via the MCP `open_search_dialog` tool, prefilling a
    // Filename-mode query and asking for autoRun. This bypasses the dialog's
    // preserved-state pitfall (where prior tests leave the query / mode dirty)
    // and the AI-mode default that would otherwise need a live provider to land
    // results.
    //
    // ❗ The SCOPE is prefilled for the same reason, and it is the load-bearing one:
    // search state survives the dialog's close, `search-open-in-pane.spec.ts` runs
    // just before this one on the same shard, and it scopes its search to "this
    // volume". Inheriting that chip pointed this run at the whole boot drive, so a
    // test that means to search a fixture tree was waiting on the root volume's index
    // coverage instead: 0.5 s when the machine was idle, past the 5 s wait when it
    // wasn't.
    const seededQuery = 'file'
    await mcpCall('open_search_dialog', {
      query: seededQuery,
      mode: 'filename',
      scope: getFixtureRoot(),
      autoRun: true,
    })

    // The footer's "Open in pane" only enables once `resultCount > 0`, so the button
    // is the observable signal that the search ran and landed results. Poll rather
    // than `waitForSelector` so a failure reports the phase the run is sitting in
    // (`data-live-phase` on the status bar) instead of a bare timeout — the whole
    // difference between "it parked waiting for another walk" and "it never started".
    await expect
      .poll(
        async () => {
          const enabled = (await tauriPage.count(OPEN_IN_PANE_BUTTON)) > 0
          if (enabled) return 'enabled'
          return await tauriPage.evaluate<string>(`(function(){
              var bar = document.querySelector('.search-overlay .status-bar');
              return 'live-phase=' + (bar ? bar.getAttribute('data-live-phase') : 'no status bar');
          })()`)
        },
        { timeout: 5000 },
      )
      .toBe('enabled')

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
