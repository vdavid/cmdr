/**
 * Search dialog: the live flow, end to end.
 *
 * The drive this runs on holds NO index while the test runs — `search-walk-ground.ts`
 * takes it away through the two per-drive actions a user has, and proves it's gone
 * before the search starts. So every row here came out of a walk: there is nothing
 * else it could have come from. (Read that module before touching either live-walk
 * spec; the E2E instance indexes its fixture tree, and a spec that forgets this
 * passes against an index read while claiming to test a walk.)
 *
 * What only an end-to-end run can prove is the wiring between the three parties: the
 * frontend mints a run id, the backend answers against it, and rows land in the list
 * WHILE the run is still going. A unit test mocks one of those away by construction.
 *
 * The stop-a-running-walk half isn't here on purpose: it's pinned at the unit tier
 * (`query-runner.streaming.test.ts`, `QueryDialog.escape.svelte.test.ts`) and end to
 * end by `search-walk-handoff.spec.ts`, which stops a run through the reopened
 * dialog.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil } from './helpers.js'
import { ensureMcpClient, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import {
  SEARCH_OVERLAY,
  closeSearchDialog,
  openSearchDialog,
  resetSearchDialog,
  setSearchInputValue,
} from './search-helpers.js'
import {
  createWalkGround,
  makeLocalVolumeUnindexed,
  removeWalkGround,
  restoreLocalVolumeIndex,
  walkGroundPath,
} from './search-walk-ground.js'

const RESULT_ROWS = `${SEARCH_OVERLAY} .result-row`
const STATUS_TEXT = `${SEARCH_OVERLAY} .status-text`
/** The walk's own progress, beside the match count. Present only while a walk runs. */
const STATUS_PROGRESS = `${SEARCH_OVERLAY} .status-progress`
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
  // The walk is deliberately throttled (`CMDR_E2E_WALK_THROTTLE_MS`) so the streaming
  // assertions have a window to happen in, and the test waits for it to finish.
  test.describe.configure({ timeout: 90_000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    createWalkGround()
  })

  test.afterAll(async () => {
    // ⚠️ An `afterAll` hook gets the CONFIG timeout (15 s), ❌ never the 90 s the
    // `describe.configure` above sets — that reaches tests, not hooks. And
    // `restoreLocalVolumeIndex` is allowed up to 50 s by its own waits, so at
    // 15 s the restore was killed mid-rebuild and every later spec in the shard
    // inherited a half-built index. Same reasoning as `search-walk-handoff`.
    test.setTimeout(90_000)
    removeWalkGround()
    await restoreLocalVolumeIndex()
  })

  test('walks the folder, streams what it finds, and says the run covered it', async ({ tauriPage }) => {
    // An empty scope box means the focused pane's current folder, so standing in the
    // walk ground is what points the search at it — the ordinary way somebody
    // searches the folder they're looking at.
    await mcpNavToPath('left', walkGroundPath())
    await openSearchDialog(tauriPage)
    await resetSearchDialog(tauriPage)

    // Take the index away LAST, once the dialog is open and quiet. It reopens holding
    // the last spec's query and re-runs it, and that run can walk — which would leave
    // this run's ground already covered and nothing to stream. Forgetting after it has
    // settled wipes whatever it wrote.
    await makeLocalVolumeUnindexed()

    await setSearchInputValue(tauriPage, 'file-*')
    await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector(${JSON.stringify(SEARCH_OVERLAY)});
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    })()`)

    // Mid-walk, in one snapshot: rows on screen, the run still stoppable, a count that
    // calls itself provisional, and the walk's own progress beside it. An index-served
    // run has none of the last three, whatever it puts in the list.
    await expect
      .poll(
        async () => {
          const rows = await tauriPage.count(RESULT_ROWS)
          const stoppable = (await tauriPage.count(STOP_BUTTON)) === 1
          const status = await textOf(tauriPage, STATUS_TEXT)
          const progress = await textOf(tauriPage, STATUS_PROGRESS)
          return rows > 0 && stoppable && status.includes('so far') && progress.includes('scanned')
        },
        { timeout: 30000 },
      )
      .toBe(true)

    // The list GROWS. Every level of the chain holds one match, so more rows arriving
    // means the walk is feeding the list rather than having handed it over at once.
    const rowsWhileWalking = await tauriPage.count(RESULT_ROWS)
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(RESULT_ROWS)) > rowsWhileWalking, 30000)).toBe(
      true,
    )

    // The run reaches a terminal state: the way to stop it goes away.
    expect(await pollUntil(tauriPage, async () => (await tauriPage.count(STOP_BUTTON)) === 0, 60000)).toBe(true)

    // Every level's file is found, so the walk covered the whole chain rather than
    // stopping wherever the assertions above happened to catch it.
    const names = await tauriPage.evaluate<string[]>(`(function(){
        return Array.from(document.querySelectorAll(${JSON.stringify(RESULT_ROWS)})).map(function (row) {
            var cell = row.querySelector('.result-name');
            return cell ? (cell.textContent || '').trim() : '';
        });
    })()`)
    expect(names.some((name) => name.includes('file-0'))).toBe(true)

    // And it says it covered its ground: the ordinary result line, NOT the lower-bound
    // one. Anything else here means the walk stopped short, which this fixture can't do.
    const status = await textOf(tauriPage, STATUS_TEXT)
    expect(status).toContain('results')
    expect(status).not.toContain("didn't finish")

    // Nothing left to caveat, so the note collapses rather than inventing a reason.
    // A walk that covered everything it was handed has nothing to report.
    expect(await textOf(tauriPage, COVERAGE_NOTE)).toBe('')

    // The announcement lives in its own region so a run's counters can be throttled
    // without freezing what the eye sees.
    expect(await tauriPage.count(LIVE_REGION)).toBe(1)
    expect(await textOf(tauriPage, LIVE_REGION)).toContain('results')

    await closeSearchDialog(tauriPage)
  })
})
