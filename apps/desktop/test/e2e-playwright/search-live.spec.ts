/**
 * Search dialog: the live flow, end to end.
 *
 * The E2E instance runs against a fresh `CMDR_DATA_DIR`, so nothing under
 * `CMDR_E2E_START_PATH` is indexed — which makes the whole fixture tree a reachable
 * frontier, the exact condition this feature exists for. Pressing Enter therefore
 * doesn't read an index; it WALKS the folder, streams what it finds, and reports how
 * the walk ended.
 *
 * What only an end-to-end run can prove is the wiring between the three parties: the
 * frontend mints a run id, the backend answers against it, and rows land in the list
 * while the run is still going. A unit test mocks one of those away by construction.
 *
 * The stop-a-running-walk half isn't here on purpose: this fixture walks in
 * milliseconds, so cancelling it would be a race. It's pinned at the unit tier
 * (`query-runner.streaming.test.ts`, `QueryDialog.escape.svelte.test.ts`); a
 * deterministic slow walk needs a soft test hook, which M7 brings.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { SEARCH_OVERLAY, closeSearchDialog, openSearchDialog, setSearchInputValue } from './search-helpers.js'

const RESULT_ROWS = `${SEARCH_OVERLAY} .result-row`
const STATUS_TEXT = `${SEARCH_OVERLAY} .status-text`
const STOP_BUTTON = `${SEARCH_OVERLAY} .status-stop`
const COVERAGE_NOTE = `${SEARCH_OVERLAY} .coverage-note`
/** The status bar's throttled live region: an inner span, never the bar itself. */
const LIVE_REGION = `${SEARCH_OVERLAY} .status-bar [aria-live="polite"]`

/** Text of the first match on `selector`, whitespace collapsed. `''` when absent. */
async function textOf(tauriPage: Parameters<typeof setSearchInputValue>[0], selector: string): Promise<string> {
  return tauriPage.evaluate<string>(`(function(){
        var el = document.querySelector(${JSON.stringify(selector)});
        return el ? (el.textContent || '').replace(/\\s+/g, ' ').trim() : '';
    })()`)
}

test.describe('Search dialog: a live search over unindexed ground', () => {
  test('walks the folder, streams what it finds, and says the run covered it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await openSearchDialog(tauriPage)

    // The dialog's state survives close + reopen by design, so an earlier spec in this
    // shard can leave a scope or a mode behind. ⌘N is the sanctioned reset.
    await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector(${JSON.stringify(SEARCH_OVERLAY)});
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', metaKey: true, bubbles: true, cancelable: true }));
    })()`)

    // An empty scope box means the focused pane's current folder, which
    // `ensureAppReady` has just put back inside the fixture tree.
    await setSearchInputValue(tauriPage, 'file-a*')
    await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector(${JSON.stringify(SEARCH_OVERLAY)});
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    })()`)

    // The rows are the point: they arrive from a walk, not from an index.
    await expect.poll(async () => tauriPage.count(RESULT_ROWS), { timeout: 15000 }).toBeGreaterThan(0)
    const names = await tauriPage.evaluate<string[]>(`(function(){
        return Array.from(document.querySelectorAll(${JSON.stringify(RESULT_ROWS)})).map(function (row) {
            var cell = row.querySelector('.result-name');
            return cell ? (cell.textContent || '').trim() : '';
        });
    })()`)
    expect(names.some((name) => name.includes('file-a'))).toBe(true)

    // The run reaches a terminal state: the way to stop it goes away.
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(STOP_BUTTON)) === 0, 15000)).toBe(true)

    // And it says it covered its ground: the ordinary result line, NOT the lower-bound
    // one. Anything else here means the walk stopped short, which this fixture can't do.
    const status = await textOf(tauriPage, STATUS_TEXT)
    expect(status).toContain('results')
    expect(status).not.toContain("didn't finish")

    // Nothing left to caveat, so the note collapses rather than inventing a reason.
    expect(await textOf(tauriPage, COVERAGE_NOTE)).toBe('')

    // The announcement lives in its own region so a run's counters can be throttled
    // without freezing what the eye sees.
    expect(await tauriPage.count(LIVE_REGION)).toBe(1)
    expect(await textOf(tauriPage, LIVE_REGION)).toContain('results')

    await closeSearchDialog(tauriPage)
  })
})
