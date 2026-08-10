/**
 * Acceptance test for "Open in pane": open the search dialog, run a query,
 * click "Open in pane", and verify the right pane shows the snapshot view.
 * Then walk `⌘[` back to the previous state and `⌘]` forward to the snapshot
 * to confirm the virtual volume integrates with the existing navigation
 * history. See `lib/search/CLAUDE.md` § "Open in pane" for the contract.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil, dispatchMenuCommand, LOCAL_VOLUME_NAME, getFixtureRoot } from './helpers.js'
import { ensureMcpClient, mcpReadResource } from '../e2e-shared/mcp-client.js'
import {
  SEARCH_OVERLAY,
  SEARCH_INPUT,
  openSearchDialog,
  resetSearchDialog,
  scopeSearchToThisVolume,
  type PageLike,
} from './search-helpers.js'

/**
 * The query bar's Run button (`QueryBar.svelte` renders the house `Button`, so `.btn`).
 * The chevron beside it is `.recent-trigger`, which carries no `.btn`, so this matches
 * the Run button alone.
 */
const RUN_BUTTON = '.search-overlay .query-bar button.btn'
// The "Show all in main window" footer button promotes the result set to a snapshot
// pane. The "footer buttons always visible" rule keeps the button in the DOM even when
// there are no results (disabled); `:not([disabled])` is the "actually clickable"
// signal — without it the test races on a disabled click.
const OPEN_IN_PANE_BUTTON = '.search-overlay [aria-label="Show all in main window"]:not([disabled])'
/**
 * The right pane's content area when it's showing a search-results snapshot.
 * `FilePane.svelte` renders `SearchResultsView` inside `.content` when
 * `volumeId === 'search-results'`. The snapshot's entries are handed to
 * `FullList` via `staticEntries`. We match on the right pane's `.full-list
 * .header-row` because once `pollRightPaneVolumeId('search-results')` confirms
 * the pane is on the snapshot, a present `.header-row` means `FullList`
 * rendered (the alternative would be `.snapshot-missing`, the defensive
 * placeholder for an evicted snapshot). There's no Path column header on the
 * snapshot pane — the Name column shows the full path instead.
 */
const SNAPSHOT_PANE_PATH_HEADER = '[aria-label="Right file pane"] .full-list .header-row'

/**
 * Reads one pane's active-tab path from the MCP `cmdr://state` resource. Both
 * sides are readable because a failure needs to say WHICH pane the snapshot
 * landed in, not just that the right one missed it.
 *
 * We can't read `volumeId` from `cmdr://state`'s pane block directly:
 * `FilePane.syncPaneStateToMcp` bails out for virtual-volume views (network
 * and search-results) because their content isn't a real directory MCP
 * agents should query. So the `volumeId:` field stays stale (`root`) even
 * after the pane swaps to a search-results snapshot.
 *
 * The active-tab line IS synced (`update_pane_tabs` runs independently),
 * and it carries the path: `i:1 id:... [active] sr-1 (search-results://sr-1)`.
 * We parse the parenthesized path on the `[active]` row of the right pane's
 * `tabs:` section. Paths starting with `search-results://` map to the
 * `search-results` virtual volume; everything else is a local-volume path.
 */
async function getPaneActiveTabPath(side: 'left' | 'right'): Promise<string | null> {
  const state = await mcpReadResource('cmdr://state?compact=true')
  const header = `\n${side}:\n`
  const sideIdx = state.indexOf(header)
  if (sideIdx === -1) return null
  // The pane's block runs until the next top-level YAML key (left margin).
  // `volumes:`, `dialogs:`, etc. live further down with no leading spaces.
  // Skip past the header (which is itself a `\n[a-z]` match) before searching
  // for the next top-level key. Measure the header rather than hardcoding its
  // width: `left` and `right` differ by a character.
  const blockStart = sideIdx + header.length
  const sideBlock = state.slice(blockStart)
  const endIdx = sideBlock.search(/\n[a-z]/)
  const scoped = endIdx === -1 ? sideBlock : sideBlock.slice(0, endIdx)
  // Active-tab line: `    - i:N id:... [active] ... (<path>)`
  const m = /^\s+- i:\d+ id:\S+ \[active\][^\n]*\(([^)\n]+)\)\s*$/m.exec(scoped)
  return m?.[1] ?? null
}

/**
 * Convenience: poll until the right pane's active tab matches the expected
 * volume id. `search-results` matches any `search-results://...` path; every
 * other string is treated as an exact `volumeId` comparison against the path
 * — except that local-volume paths don't carry a volume prefix, so we accept
 * the local case by ruling out the known virtual prefixes.
 */
async function pollRightPaneVolumeId(
  tauriPage: PageLike,
  expected: string | { not: string },
  // 10 s, not 3 s: the volume id is read from `cmdr://state`, which the FE syncs on
  // a debounce after a nav. On the shared Docker VM under full-suite load (E2E +
  // rust-tests-linux + SMB containers) that sync stretched past 3 s and flaked this
  // poll. The probe still returns the instant the id matches; this is failure
  // headroom, matching `moveCursorToFile`'s 8 s precedent for the same loaded VM.
  timeoutMs = 10000,
): Promise<boolean> {
  const matches = (path: string, target: string): boolean => {
    if (target === 'search-results') return path.startsWith('search-results://')
    if (target === 'network') return path.startsWith('smb://')
    // Local volume: anything not on a known virtual-volume prefix.
    return !path.startsWith('search-results://') && !path.startsWith('smb://')
  }
  return pollUntil(
    tauriPage,
    async () => {
      const path = await getPaneActiveTabPath('right')
      if (path === null) return false
      if (typeof expected === 'string') return matches(path, expected)
      return !matches(path, expected.not)
    },
    timeoutMs,
  )
}

/**
 * Asserts the right pane reaches `expected`, and FAILS WITH EVIDENCE.
 *
 * `expect(await pollRightPaneVolumeId(...)).toBe(true)` reports "Expected: true,
 * Received: false" and nothing else, which is why this spec's flake survived two
 * fix attempts: the message can't distinguish "the snapshot landed in the LEFT
 * pane" from "no snapshot was created" from "the `cmdr://state` sync lagged past
 * the deadline". Those need different fixes. So on failure this reports where BOTH
 * panes actually ended up, and the CI artifact upload now keeps the page snapshot
 * beside it (`ci.yml`, "Upload E2E screenshots on failure").
 */
async function expectRightPaneVolumeId(
  tauriPage: PageLike,
  expected: string | { not: string },
  timeoutMs = 10000,
): Promise<void> {
  if (await pollRightPaneVolumeId(tauriPage, expected, timeoutMs)) return
  const [right, left] = [await getPaneActiveTabPath('right'), await getPaneActiveTabPath('left')]
  const wanted = typeof expected === 'string' ? expected : `anything but ${expected.not}`
  throw new Error(
    `Right pane never reached ${wanted} within ${String(timeoutMs)} ms.\n` +
      `  right pane active tab: ${right ?? '(unreadable)'}\n` +
      `  left pane active tab:  ${left ?? '(unreadable)'}\n` +
      `If the left pane holds the search-results path, the promotion targeted the wrong pane ` +
      `(a focus race), not the navigation history.`,
  )
}

/** Convenience: poll for the search overlay to unmount. */
async function pollOverlayGone(tauriPage: PageLike, timeoutMs = 10000): Promise<boolean> {
  // 10 s for the same loaded-Docker-VM headroom as pollRightPaneVolumeId; the
  // overlay-unmount tick after the Open-in-pane click can stretch under load.
  return pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 0, timeoutMs)
}

/**
 * Idempotently focuses the right pane, and proves it from the DOM.
 *
 * "Open in pane" targets `focusedPane` (`DualPaneExplorer.openSearchSnapshotInPane`),
 * which is FRONTEND state; `.file-pane.is-focused` renders straight off it, so the
 * class is the authoritative read. ❌ Don't steer by `cmdr://state`'s `focused:`
 * field: that's a separate backend mirror, written only by `handleFocus` /
 * `switchPane` through the fire-and-forget `updateFocusedPane` IPC. The MCP
 * listeners this spec's own setup drives (`mcp-volume-select`, `mcp-nav-to-path`)
 * shift FE focus through the store-only `setFocusedPane`, deliberately leaving the
 * mirror to the backend's `nav_to_path` tool — which a test emitting the FE event
 * directly never goes through. So after `resetRightPaneToLocalIfNeeded` the FE is
 * on the right pane while the mirror still says `left`, and a helper that reads the
 * mirror and answers with a TOGGLE (`pane.switch`) moves focus the WRONG way, then
 * can return happy on a stale read with the LEFT pane focused. The snapshot lands
 * left and the test fails three assertions later, in the navigation-history step.
 *
 * Clicking the pane is idempotent and needs no reading of anything: it routes
 * through `handlePaneClick` → `onRequestFocus` → `handleFocus('right')`, the same
 * path `ensureAppReady` uses to claim the left pane.
 */
async function focusRightPane(tauriPage: PageLike): Promise<void> {
  const focused = await pollUntil(
    tauriPage,
    async () =>
      tauriPage.evaluate<boolean>(`(function() {
            var right = document.querySelector('[aria-label="Right file pane"]');
            if (!right) return false;
            // Svelte applies the class on the next tick, so the click's effect is
            // observed by the NEXT poll iteration, not this return.
            if (!right.classList.contains('is-focused')) right.click();
            return right.classList.contains('is-focused');
        })()`),
    5000,
  )
  if (!focused) {
    const diag = await tauriPage.evaluate<string>(`(function() {
            var panes = document.querySelectorAll('.file-pane');
            return JSON.stringify({
                paneCount: panes.length,
                focusedPaneIndex: Array.from(panes).findIndex(function(p){ return p.classList.contains('is-focused'); }),
                rightPaneFound: !!document.querySelector('[aria-label="Right file pane"]'),
            });
        })()`)
    throw new Error(`Failed to focus the right pane within 5000 ms. State: ${diag}`)
  }
}

/**
 * Resets the right pane to the local volume + fixture path if a previous
 * test left it on the `search-results://` virtual volume. `ensureAppReady`
 * skips this on its own: the FilePane's `syncPaneStateToMcp` bails out for
 * virtual-volume panes, so `cmdr://state` still reports
 * `volume: Macintosh HD` and the `isStateClean` short-circuit fires. The
 * active-tab line, which IS synced, shows the truth. We emit
 * `mcp-volume-select` for the right pane when its active tab is on a
 * snapshot path, then nav back to the fixture so `⌘[` from a fresh
 * Open-in-pane lands somewhere meaningful instead of on the volume root.
 */
async function resetRightPaneToLocalIfNeeded(
  tauriPage: PageLike,
  localVolumeName: string,
  fixtureRightPath: string,
): Promise<void> {
  const path = await getPaneActiveTabPath('right')
  if (path === null || !path.startsWith('search-results://')) return
  // Swap volumes off the snapshot first, then nav back to the fixture's
  // right directory. Without the explicit nav, the reset lands on the
  // volume's root (`/` on macOS), which is also valid local-volume state
  // but means `⌘[` from a later snapshot lands on `/` rather than on the
  // fixture path the test was designed around.
  await tauriPage.evaluate(`(function(){
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', {
            event: 'mcp-volume-select',
            payload: { pane: 'right', name: ${JSON.stringify(localVolumeName)} }
        });
    })()`)
  await expect
    .poll(
      async () => {
        const p = await getPaneActiveTabPath('right')
        return p !== null && !p.startsWith('search-results://')
      },
      { timeout: 3000 },
    )
    .toBeTruthy()
  await tauriPage.evaluate(`(function(){
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'right', path: ${JSON.stringify(fixtureRightPath)} }
        });
    })()`)
  await expect
    .poll(
      async () => {
        const p = await getPaneActiveTabPath('right')
        return p === fixtureRightPath
      },
      { timeout: 3000 },
    )
    .toBeTruthy()
}

/**
 * Types into the search input and waits for results to land. The dialog
 * auto-applies on a 1 s debounce in filename / regex modes; the Run button is
 * the synchronous path that bypasses the debounce, which is what we want for a
 * deterministic test.
 *
 * ❌ Don't press Enter here. `⏎` ownership swaps (`deriveEnterAction`): once
 * results exist AND the last dialog event is `results-arrived` / `cursor-moved`,
 * bare Enter means "go to file", which closes the dialog and navigates the pane.
 * Reopening the dialog in a session that already ran a query re-runs it on mount,
 * so those results can land AFTER this helper's `input` event and flip ownership
 * out from under the keypress; the run then never happens and the wait below
 * times out on a dialog that just closed. `runFromButton` has no such ambiguity.
 * (`⏎` ownership itself is pinned by `enter-action.test.ts` and the QueryDialog
 * Vitest suite, so nothing is lost by not exercising it here.)
 */
async function typeAndRunSearch(tauriPage: PageLike, query: string): Promise<void> {
  // ⌘N first, so the run this helper starts is the ONLY one on screen. The dialog's
  // state survives close + reopen by design and it re-runs the carried-over query on
  // its own, so without the reset the `OPEN_IN_PANE_BUTTON` wait below can be satisfied
  // by the LEFTOVER run's rows. The click then lands while the run under test is still
  // arriving — the button flickers back to `[disabled]` as `resultCount` resets — so no
  // snapshot is created and the caller's `pollRightPaneVolumeId('search-results')`
  // times out, reading as "nav history is broken" instead of "the click missed".
  await resetSearchDialog(tauriPage)

  // The fixtures this spec matches (`file-a.txt`, …) live under `<root>/left`, but the
  // spec focuses the RIGHT pane so "Open in pane" targets it — and an unset scope now
  // means the focused pane's current folder. Widen to the volume, or the search runs in
  // the empty `<root>/right` and the Open-in-pane button never enables. ⌘N resets the
  // scope too, so this has to come after it.
  await scopeSearchToThisVolume(tauriPage)

  // Returns whether the input was there. A swallowed miss would leave the box holding
  // whatever an earlier spec typed and run THAT query, which still lands rows and still
  // enables the button — a green test measuring the wrong search.
  const typed = await tauriPage.evaluate<boolean>(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        if (!el) return false;
        el.focus();
        el.value = ${JSON.stringify(query)};
        el.dispatchEvent(new Event('input', { bubbles: true }));
        return true;
    })()`)
  expect(typed).toBe(true)
  await expect
    .poll(
      async () => tauriPage.evaluate<string>(`document.querySelector(${JSON.stringify(SEARCH_INPUT)})?.value ?? ''`),
      { timeout: 3000 },
    )
    .toBe(query)

  await tauriPage.click(RUN_BUTTON)
  // The footer's "Open in pane" only enables once `resultCount > 0`. Waiting
  // for the button is the observable signal that the search ran and landed
  // results, no magic timer. After the ⌘N above, the only run that can enable it
  // is this one.
  await tauriPage.waitForSelector(OPEN_IN_PANE_BUTTON, 5000)
}

test.describe('Search dialog: Open in pane', () => {
  test('Open in pane lands the right pane on a search-results snapshot', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await resetRightPaneToLocalIfNeeded(tauriPage, LOCAL_VOLUME_NAME, `${getFixtureRoot()}/right`)

    // Focus the right pane so "Open in pane" targets it: the promotion routes to
    // `focusedPane`, and prior tests (plus the reset above) can leave focus on
    // either side. `focusRightPane` confirms it from the DOM before we open the
    // dialog, so the handoff can't read a pane we only THINK is focused.
    await focusRightPane(tauriPage)

    await openSearchDialog(tauriPage)

    // Type a query that matches the fixture (`file-a.txt`, `file-b.txt` etc.)
    // and run it synchronously via Enter. The dialog focuses the input on
    // mount, so we don't need to focus it explicitly.
    await typeAndRunSearch(tauriPage, 'file')

    // Click "Open in pane". The dialog closes and the right pane swaps to
    // the search-results virtual volume.
    await tauriPage.click(OPEN_IN_PANE_BUTTON)

    expect(await pollOverlayGone(tauriPage)).toBe(true)
    await expectRightPaneVolumeId(tauriPage, 'search-results')

    // The path column header is the shrink-wrapped marker for the search-results pane
    // (FullList + showPathColumn). Confirms the view actually rendered and isn't a
    // placeholder.
    const pathHeaderCount = await tauriPage.count(SNAPSHOT_PANE_PATH_HEADER)
    expect(pathHeaderCount).toBeGreaterThan(0)
  })

  test('⌘[ leaves the snapshot view, ⌘] returns to it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await resetRightPaneToLocalIfNeeded(tauriPage, LOCAL_VOLUME_NAME, `${getFixtureRoot()}/right`)
    await focusRightPane(tauriPage)
    await openSearchDialog(tauriPage)
    await typeAndRunSearch(tauriPage, 'file')
    await tauriPage.click(OPEN_IN_PANE_BUTTON)

    expect(await pollOverlayGone(tauriPage)).toBe(true)
    await expectRightPaneVolumeId(tauriPage, 'search-results')

    // ⌘[ goes back. The right pane's history landed an entry for the
    // previous local-volume path before the snapshot, so back must leave
    // the snapshot view. Route through `dispatchMenuCommand('nav.back')`
    // rather than synthesizing the key combo — `pressKey` dispatches the
    // keydown on `document.activeElement`, which after the Open-in-pane
    // click can be on the (now-unmounted) overlay button, dropping the
    // event before it bubbles to `handleGlobalKeyDown`. The Tauri-event
    // path is direct and immune to that race.
    await dispatchMenuCommand(tauriPage, 'nav.back')
    await expectRightPaneVolumeId(tauriPage, { not: 'search-results' })

    // ⌘] goes forward, back to the snapshot.
    await dispatchMenuCommand(tauriPage, 'nav.forward')
    await expectRightPaneVolumeId(tauriPage, 'search-results')
  })
})
