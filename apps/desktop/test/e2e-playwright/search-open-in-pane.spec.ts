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
import {
  ensureAppReady,
  pressKey,
  pollUntil,
  dispatchMenuCommand,
  LOCAL_VOLUME_NAME,
  getFixtureRoot,
} from './helpers.js'
import { ensureMcpClient, mcpReadResource } from '../e2e-shared/mcp-client.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const SEARCH_OVERLAY = '.search-overlay'
const SEARCH_INPUT = '.search-overlay input'
const OPEN_IN_PANE_BUTTON = '.search-overlay [aria-label="Open in pane"]'
/**
 * The right pane's content area when it's showing a search-results snapshot.
 * `FilePane.svelte` renders `SearchResultsView` (rooted in a `.full-list`
 * container with the path column) inside `.content` only when
 * `volumeId === 'search-results'`. We match on the Path column header
 * (`.header-path`, set in `FullList.svelte` when `showPathColumn` is true)
 * because it's unique to the snapshot view and stable across results.
 */
const SNAPSHOT_PANE_PATH_HEADER = '.file-pane .full-list .header-path'

async function openSearchDialog(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'search.open')
  await tauriPage.waitForSelector(SEARCH_OVERLAY, 3000)
}

/**
 * Reads the right pane's active-tab path from the MCP `cmdr://state` resource.
 *
 * We can't read `volumeId` from `cmdr://state`'s `right:` block directly:
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
async function getRightPaneActiveTabPath(): Promise<string | null> {
  const state = await mcpReadResource('cmdr://state?compact=true')
  const rightIdx = state.indexOf('\nright:\n')
  if (rightIdx === -1) return null
  // The `right:` block runs until the next top-level YAML key (left margin).
  // `volumes:`, `dialogs:`, etc. live further down with no leading spaces.
  // Skip past `\nright:\n` (which is itself a `\n[a-z]` match) before
  // searching for the next top-level key.
  const blockStart = rightIdx + '\nright:\n'.length
  const rightBlock = state.slice(blockStart)
  const endIdx = rightBlock.search(/\n[a-z]/)
  const scoped = endIdx === -1 ? rightBlock : rightBlock.slice(0, endIdx)
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
  timeoutMs = 3000,
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
      const path = await getRightPaneActiveTabPath()
      if (path === null) return false
      if (typeof expected === 'string') return matches(path, expected)
      return !matches(path, expected.not)
    },
    timeoutMs,
  )
}

/** Convenience: poll for the search overlay to unmount. */
async function pollOverlayGone(tauriPage: PageLike, timeoutMs = 3000): Promise<boolean> {
  return pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 0, timeoutMs)
}

/**
 * Polls `cmdr://state` until the `focused:` field matches `expected`.
 * The Tab key dispatches `pane.switch` through the command system; this
 * guards against running the dialog before the focus flip lands.
 */
async function pollFocusedPane(tauriPage: PageLike, expected: 'left' | 'right', timeoutMs = 3000): Promise<boolean> {
  return pollUntil(
    tauriPage,
    async () => {
      const state = await mcpReadResource('cmdr://state?compact=true')
      const m = /^focused:\s*(\S+)/m.exec(state)
      return m?.[1] === expected
    },
    timeoutMs,
  )
}

/**
 * Idempotently focuses the right pane. Reads `cmdr://state.focused`; dispatches
 * `pane.switch` via the command system if it isn't already on the right.
 *
 * Previously this pressed `Tab`, but a bare Tab keypress is brittle: it only
 * dispatches `pane.switch` when `document.activeElement` is inside the file
 * explorer, and prior tests can leave focus on a dialog overlay or an input.
 * Routing through `dispatchMenuCommand` is the same command path the F-key bar
 * and the menu use; it works regardless of where DOM focus currently is.
 */
async function focusRightPane(tauriPage: PageLike): Promise<void> {
  for (let attempt = 0; attempt < 4; attempt++) {
    if (await pollFocusedPane(tauriPage, 'right', 1000)) return
    await dispatchMenuCommand(tauriPage, 'pane.switch')
  }
  throw new Error('Failed to focus right pane after retries')
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
  const path = await getRightPaneActiveTabPath()
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
  await pollUntil(
    tauriPage,
    async () => {
      const p = await getRightPaneActiveTabPath()
      return p !== null && !p.startsWith('search-results://')
    },
    3000,
  )
  await tauriPage.evaluate(`(function(){
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'right', path: ${JSON.stringify(fixtureRightPath)} }
        });
    })()`)
  await pollUntil(
    tauriPage,
    async () => {
      const p = await getRightPaneActiveTabPath()
      return p === fixtureRightPath
    },
    3000,
  )
}

/**
 * Types into the search input and waits for results to land. The dialog
 * auto-applies on a 1 s debounce in filename / regex modes (M6); pressing
 * Enter is the synchronous path that bypasses the debounce, which is what
 * we want for a deterministic test.
 *
 * We set `.value` directly and fire `input` so the bound query state
 * updates, then `pressKey('Enter')` dispatches a real keydown event on the
 * focused input. The dialog's overlay-level handler reads
 * `document.activeElement === queryInputElement` to decide whether to run
 * `executeSearch`, so the input MUST be the active element. The dialog
 * focuses its input on mount, but reopens during the same session can race
 * with onMount focus; we click the input first to be deterministic.
 */
async function typeAndRunSearch(tauriPage: PageLike, query: string): Promise<void> {
  await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        if (!el) return;
        el.focus();
        el.value = ${JSON.stringify(query)};
        el.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
  await pressKey(tauriPage, 'Enter')
  // The footer's "Open in pane" only renders once `resultCount > 0`. Waiting
  // for the button is the observable signal that the search ran and landed
  // results, no magic timer.
  await tauriPage.waitForSelector(OPEN_IN_PANE_BUTTON, 5000)
}

test.describe('Search dialog: Open in pane (M8c acceptance)', () => {
  test('Open in pane lands the right pane on a search-results snapshot', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await resetRightPaneToLocalIfNeeded(tauriPage, LOCAL_VOLUME_NAME, `${getFixtureRoot()}/right`)

    // Focus the right pane so "Open in pane" targets it. Tab toggles the
    // focused pane via `pane.switch`; press only when needed since prior
    // tests in the same session can leave focus on either side. We poll
    // `cmdr://state.focused` to confirm the swap before opening the dialog
    // so the dialog's `onOpenInPane` handoff reads the right `focusedPane`.
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
    expect(await pollRightPaneVolumeId(tauriPage, 'search-results')).toBe(true)

    // The path column header is the M8b shrink-wrapped marker for the
    // search-results pane (FullList + showPathColumn). Confirms the view
    // actually rendered and isn't a placeholder.
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
    expect(await pollRightPaneVolumeId(tauriPage, 'search-results')).toBe(true)

    // ⌘[ goes back. The right pane's history landed an entry for the
    // previous local-volume path before the snapshot, so back must leave
    // the snapshot view. Route through `dispatchMenuCommand('nav.back')`
    // rather than synthesizing the key combo — `pressKey` dispatches the
    // keydown on `document.activeElement`, which after the Open-in-pane
    // click can be on the (now-unmounted) overlay button, dropping the
    // event before it bubbles to `handleGlobalKeyDown`. The Tauri-event
    // path is direct and immune to that race.
    await dispatchMenuCommand(tauriPage, 'nav.back')
    expect(await pollRightPaneVolumeId(tauriPage, { not: 'search-results' })).toBe(true)

    // ⌘] goes forward, back to the snapshot.
    await dispatchMenuCommand(tauriPage, 'nav.forward')
    expect(await pollRightPaneVolumeId(tauriPage, 'search-results')).toBe(true)
  })
})
