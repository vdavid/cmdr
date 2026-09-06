/**
 * "Open in pane" while the search is still walking: the pane keeps filling.
 *
 * The drive holds NO index while these tests run, and the ground is a directory
 * chain a throttled walk can't get through quickly — both from
 * `search-walk-ground.ts`, which explains why an E2E instance needs either. Read it
 * before touching this spec: without the first half there is no walk at all (the
 * fixture tree is indexed at launch), and without the second there is no walk still
 * running by the time the dialog closes.
 *
 * What only an end-to-end run can prove is the handoff itself — that the walk
 * survives the dialog closing, that its rows reach a snapshot nobody is holding open
 * in a dialog, and that the toast is there saying so. Every unit test in
 * `walk-handoff.svelte.test.ts` mocks one of those three away by construction.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil, LOCAL_VOLUME_NAME, getFixtureRoot } from './helpers.js'
import { ensureMcpClient, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import {
  SEARCH_OVERLAY,
  SEARCH_INPUT,
  closeSearchDialog,
  openSearchDialog,
  resetSearchDialog,
  type PageLike,
} from './search-helpers.js'
import {
  createWalkGround,
  makeLocalVolumeUnindexed,
  removeWalkGround,
  restoreLocalVolumeIndex,
  walkGroundPath,
} from './search-walk-ground.js'

/** The footer button that promotes the result set into a pane, once it has rows. */
const OPEN_IN_PANE_BUTTON = '.search-overlay [aria-label="Show all in main window"]:not([disabled])'
/** The status bar's Stop button: present exactly while a live run is going. */
const STOP_BUTTON = `${SEARCH_OVERLAY} .status-stop`
/**
 * Rows in the LEFT pane, which is where the snapshot lands: "Show all in main window"
 * routes the ACTIVE pane to `search-results://<id>`, and `ensureAppReady` leaves the
 * left one active.
 *
 * `FullList` virtualizes, so this counts what's RENDERED. The chain's matches fit on
 * one screen, which is what lets a growing count mean a growing snapshot here.
 */
const SNAPSHOT_ROWS = '[aria-label="Left file pane"] .full-list .file-entry'
/** Every toast on screen, whatever it says. */
const TOASTS = '.toast'

/**
 * Clears whatever the run's last word was.
 *
 * The suite's global `afterEach` fails a test that leaks a toast, and both tests here
 * end on one by design: a search that finishes says so, and a stopped one admits it
 * came back short.
 */
async function dismissEveryToast(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
        var closes = document.querySelectorAll('.toast .toast-close');
        for (var i = 0; i < closes.length; i++) closes[i].click();
    })()`)
  await expect.poll(async () => tauriPage.count(TOASTS), { timeout: 5000 }).toBe(0)
}

/** All toast text on screen, whitespace collapsed. */
async function toastText(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(function(){
        return Array.from(document.querySelectorAll(${JSON.stringify(TOASTS)}))
            .map(function (el) { return (el.textContent || '').replace(/\\s+/g, ' ').trim() })
            .join(' | ');
    })()`)
}

/**
 * Stands in the walk ground and starts a live run over it, leaving the dialog open
 * with rows arriving and the walk still going.
 *
 * An empty scope box means the focused pane's folder, so navigating there is what
 * points the search at the chain.
 */
async function startWalkingSearch(tauriPage: PageLike): Promise<void> {
  await mcpNavToPath('left', walkGroundPath())
  await openSearchDialog(tauriPage)

  await resetSearchDialog(tauriPage)

  // Take the index away LAST, once the dialog is open and quiet. It reopens holding
  // the last spec's query and re-runs it, and that run can walk — which would leave
  // this run's ground already covered and the walk over before the dialog closes.
  // Forgetting after it has settled wipes whatever it wrote.
  await makeLocalVolumeUnindexed()

  await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        if (!el) return;
        el.focus();
        el.value = 'file-*';
        el.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
  await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector(${JSON.stringify(SEARCH_OVERLAY)});
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    })()`)

  // Rows are arriving AND the run is still going: both have to be true at the moment
  // we promote, or the test proves nothing about a walk outliving its dialog.
  await tauriPage.waitForSelector(OPEN_IN_PANE_BUTTON, 30000)
  expect(await tauriPage.count(STOP_BUTTON)).toBe(1)
}

test.describe('Search dialog: a walk that outlives its dialog', () => {
  // The walk is deliberately throttled (`CMDR_E2E_WALK_THROTTLE_MS`) so the handoff
  // happens mid-walk, and the first test waits for that walk to finish on its own.
  test.describe.configure({ timeout: 90_000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    createWalkGround()
  })

  test.afterAll(async () => {
    // ⚠️ An `afterAll` hook gets the CONFIG timeout (15 s), ❌ never the 90 s the
    // `describe.configure` above sets — that reaches tests, not hooks. And
    // `restoreLocalVolumeIndex` is allowed up to 50 s by its own waits (30 s for
    // `fresh`, then 20 s for an answer a search can actually read). At 15 s the
    // restore was killed mid-rebuild and the shared app carried a half-built
    // index into every later spec in the shard: CI run 33909203247 lost nine
    // specs downstream of exactly that, all reported as their own failures.
    test.setTimeout(90_000)
    removeWalkGround()
    await restoreLocalVolumeIndex()
  })

  test('keeps filling the pane, and says so in a toast', async ({ tauriPage }) => {
    await startWalkingSearch(tauriPage)

    await tauriPage.click(OPEN_IN_PANE_BUTTON)
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 0, 10000)).toBe(true)

    // The toast is the ONLY thing on screen saying the search is still running, so
    // it's the whole interface for that state.
    await expect.poll(async () => toastText(tauriPage), { timeout: 10000 }).toContain('Still searching')

    const rowsAtHandoff = await tauriPage.count(SNAPSHOT_ROWS)
    expect(rowsAtHandoff).toBeGreaterThan(0)

    // The point of the milestone: the pane grows with the dialog gone. Snapshots
    // aren't reactive, so this also proves the `mutationTick` bump — without it the
    // rows would land in the store and never reach the screen.
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(SNAPSHOT_ROWS)) > rowsAtHandoff, 30000)).toBe(
      true,
    )

    // And it settles on its own: the running toast gives way, and what replaces it is
    // transient, so nothing is left holding the screen.
    await expect.poll(async () => toastText(tauriPage), { timeout: 60000 }).not.toContain('Still searching')
    await dismissEveryToast(tauriPage)
  })

  test('reopening the dialog shows the running search rather than its leftovers', async ({ tauriPage }) => {
    await startWalkingSearch(tauriPage)

    await tauriPage.click(OPEN_IN_PANE_BUTTON)
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 0, 10000)).toBe(true)
    await expect.poll(async () => toastText(tauriPage), { timeout: 10000 }).toContain('Still searching')

    // Reopening adopts the run instead of starting a fresh one. A fresh one would
    // SUPERSEDE the live walk and the pane would quietly stop growing, so the Stop
    // button being here is the proof that the dialog is looking at the same search.
    await openSearchDialog(tauriPage)
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(STOP_BUTTON)) === 1, 10000)).toBe(true)

    // Escape's first press stops the walk, the second closes the dialog.
    await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector(${JSON.stringify(SEARCH_OVERLAY)});
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    })()`)
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(STOP_BUTTON)) === 0, 10000)).toBe(true)
    await closeSearchDialog(tauriPage)
    await dismissEveryToast(tauriPage)

    // Housekeeping for the specs that follow: leave the left pane off the snapshot
    // volume, standing somewhere that still exists once the walk ground is removed.
    await tauriPage.evaluate(`(function(){
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', {
            event: 'mcp-volume-select',
            payload: { pane: 'left', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} }
        });
        invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'left', path: ${JSON.stringify(`${getFixtureRoot()}/left`)} }
        });
    })()`)
  })
})
