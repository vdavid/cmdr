/**
 * M8 acceptance test for the search-redesign plan: open the search dialog,
 * run a query, click "Open in pane", and verify the right pane shows the
 * snapshot view. Then walk `⌘[` back to the previous state and `⌘]` forward
 * to the snapshot to confirm the virtual volume integrates with the existing
 * navigation history.
 *
 * Spec name from search-redesign-plan §M8c.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, pressKey, pollUntil, dispatchMenuCommand, CTRL_OR_META } from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const SEARCH_OVERLAY = '.search-overlay'
const SEARCH_INPUT = '.search-overlay input'
const OPEN_IN_PANE_BUTTON = '.search-overlay [aria-label="Open in pane"]'
/**
 * The right pane's content area when it's showing a search-results snapshot.
 * `FilePane.svelte` renders `SearchResultsView` (rooted in a `.full-list`
 * container with the path column) inside `.content` only when
 * `volumeId === 'search-results'`. We match on the Path column header — added
 * by the M8b path-column work — because it's unique to the snapshot view and
 * stable across results.
 */
const SNAPSHOT_PANE_PATH_HEADER = '.file-pane .full-list .col-path-header'

async function openSearchDialog(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'search.open')
  await tauriPage.waitForSelector(SEARCH_OVERLAY, 3000)
}

/** Reads the right pane's `volumeId` from `cmdr://state`. */
async function getRightPaneVolumeId(tauriPage: PageLike): Promise<string | null> {
  const json = await tauriPage.evaluate<string>(`(async function(){
        var inv = window.__TAURI_INTERNALS__.invoke;
        var s = await inv('get_app_state');
        return typeof s === 'string' ? s : JSON.stringify(s);
    })()`)
  const m = /"right"\s*:\s*\{[^}]*"volumeId"\s*:\s*"([^"]+)"/.exec(json)
  return m?.[1] ?? null
}

/** Convenience: poll until the right pane reports a specific `volumeId`. */
async function pollRightPaneVolumeId(
  tauriPage: PageLike,
  expected: string | { not: string },
  timeoutMs = 3000,
): Promise<boolean> {
  return pollUntil(
    tauriPage,
    async () => {
      const vid = await getRightPaneVolumeId(tauriPage)
      if (vid === null) return false
      if (typeof expected === 'string') return vid === expected
      return vid !== expected.not
    },
    timeoutMs,
  )
}

/** Convenience: poll for the search overlay to unmount. */
async function pollOverlayGone(tauriPage: PageLike, timeoutMs = 3000): Promise<boolean> {
  return pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 0, timeoutMs)
}

test.describe('Search dialog: Open in pane (M8c acceptance)', () => {
  test('Open in pane lands the right pane on a search-results snapshot', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Focus the right pane so "Open in pane" targets it. Tab switches the
    // focused pane via DualPaneExplorer's keyboard handler.
    await pressKey(tauriPage, 'Tab')

    await openSearchDialog(tauriPage)

    // Type a query that matches the fixture (`file-a.txt`, `file-b.txt` etc.).
    // The dialog focuses the input on mount.
    await tauriPage.evaluate(`(function(){
            var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
            if (!el) return;
            el.value = 'file';
            el.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)

    // Wait for results to land. The footer's "Open in pane" only renders
    // when `resultCount > 0`.
    await tauriPage.waitForSelector(OPEN_IN_PANE_BUTTON, 5000)

    // Click "Open in pane". The dialog closes and the right pane swaps to
    // the search-results virtual volume.
    await tauriPage.click(OPEN_IN_PANE_BUTTON)

    expect(await pollOverlayGone(tauriPage)).toBe(true)
    expect(await pollRightPaneVolumeId(tauriPage, 'search-results')).toBe(true)

    // The path column header is the M8b shrink-wrapped marker for the
    // search-results pane (FullList + showPathColumn). Confirms the view
    // actually rendered and isn't a placeholder.
    const pathHeaderCount = await tauriPage.count(SNAPSHOT_PANE_PATH_HEADER)
    expect(pathHeaderCount).toBeGreaterThan(0)
  })

  test('⌘[ leaves the snapshot view, ⌘] returns to it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await pressKey(tauriPage, 'Tab')
    await openSearchDialog(tauriPage)
    await tauriPage.evaluate(`(function(){
            var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
            if (!el) return;
            el.value = 'file';
            el.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)
    await tauriPage.waitForSelector(OPEN_IN_PANE_BUTTON, 5000)
    await tauriPage.click(OPEN_IN_PANE_BUTTON)

    expect(await pollOverlayGone(tauriPage)).toBe(true)
    expect(await pollRightPaneVolumeId(tauriPage, 'search-results')).toBe(true)

    // ⌘[ goes back. The right pane's history landed an entry for the
    // previous local-volume path before the snapshot, so back must leave
    // the snapshot view.
    await pressKey(tauriPage, `${CTRL_OR_META}+BracketLeft`)
    expect(await pollRightPaneVolumeId(tauriPage, { not: 'search-results' })).toBe(true)

    // ⌘] goes forward, back to the snapshot.
    await pressKey(tauriPage, `${CTRL_OR_META}+BracketRight`)
    expect(await pollRightPaneVolumeId(tauriPage, 'search-results')).toBe(true)
  })
})
