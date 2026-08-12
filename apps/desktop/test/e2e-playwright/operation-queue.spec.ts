/**
 * E2E for the operation-queue window.
 *
 * Two same-lane local copies serialize behind the operation manager (lane budget
 * 1 per device, and both copies touch the local volume's lane): the first runs,
 * the second queues. The queue window then shows one Running + one Queued row.
 * From there we cancel the queued one (it drops) and pause + resume the running
 * one (its status flips to Paused, then back to Running).
 *
 * The copies are kicked off directly through the `copy_between_volumes` IPC (the
 * same command the F5 dialog calls), which registers the op and returns its id
 * immediately — no modal needed.
 *
 * Each copy source is a dedicated multi-file directory created per test (NOT the
 * shared `bulk/` tree, which other specs mutate). Two reasons it must be a
 * directory of many small files, not one big file:
 *   1. The E2E copy throttle (`set_test_throttle`) sleeps once PER FILE, and
 *      local APFS copies clone whole-file (no per-chunk hook), so a single-file
 *      copy lives only ~one throttle tick — far too short to observe Running /
 *      Queued or to drive the cancel → pause → resume sequence.
 *   2. Pause gates BETWEEN files; a one-file copy has no between-files gate, so
 *      it can't be paused at all. Many files give pause a place to land.
 *
 * Requires `--features playwright-e2e`.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  closeScopedWindow,
  ensureAppReady,
  expectAndDismissToast,
  getFixtureRoot,
  moveCursorToFile,
  TRANSFER_DIALOG,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const QUEUE_LABEL = 'queue'
/** The progress dialog (NOT the destination picker `TRANSFER_DIALOG`). */
const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'

/** Per-file copy throttle. With `FILES_PER_SOURCE` files per op this keeps each
 *  copy in flight for ~`FILES_PER_SOURCE * THROTTLE_MS` ms, leaving generous room
 *  to observe states and drive pause/resume even on the slow Docker VM. */
const THROTTLE_MS = 250
/** Enough files that an op stays Running across the whole cancel/pause/resume
 *  sequence (the poll budgets resolve early on a green run, so the headroom is
 *  free). */
const FILES_PER_SOURCE = 24

/** Two distinct source dirs so the two copies never conflict on the destination
 *  (they share the local lane, so they still serialize: one Running, one Queued). */
const SOURCE_A = 'queue-src-a'
const SOURCE_B = 'queue-src-b'
/** Deliberately never created: a copy from here fails validation. */
const MISSING_SOURCE = 'queue-src-gone'

test.setTimeout(90_000)

/** Creates `left/<name>/` with `FILES_PER_SOURCE` tiny files. Node-side (real
 *  disk), mirroring conflict-edge-cases.spec.ts's own-fixture pattern. */
function makeSourceDir(fixtureRoot: string, name: string): void {
  const dir = path.join(fixtureRoot, 'left', name)
  fs.mkdirSync(dir, { recursive: true })
  for (let i = 0; i < FILES_PER_SOURCE; i++) {
    fs.writeFileSync(path.join(dir, `file-${String(i).padStart(2, '0')}.txt`), 'x'.repeat(1024))
  }
}

/** Starts a local→local copy of `left/<sourceName>/` into `right/` via the
 *  production IPC. Returns nothing; the op registers and the manager admits or
 *  queues it. */
async function startCopy(tauriPage: TauriPage, fixtureRoot: string, sourceName: string): Promise<void> {
  const src = JSON.stringify(`${fixtureRoot}/left/${sourceName}`)
  const destDir = JSON.stringify(`${fixtureRoot}/right`)
  // `copy_between_volumes` args (camelCase): sourceVolumeId, sourcePaths,
  // destVolumeId, destPath, config. Both volumes are the default local "root",
  // so the two copies share the local lane and serialize.
  await tauriPage.evaluate(`(async function() {
    await window.__TAURI_INTERNALS__.invoke('copy_between_volumes', {
      sourceVolumeId: 'root',
      sourcePaths: [${src}],
      destVolumeId: 'root',
      destPath: ${destDir},
      config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] }
    });
  })()`)
}

/** Starts a copy that CANNOT succeed: the source doesn't exist, so validation
 *  fails inside the spawned operation and the backend emits `write-error` for a
 *  real, registered op. The cheapest deterministic failure there is. */
async function startDoomedCopy(tauriPage: TauriPage, fixtureRoot: string): Promise<void> {
  const src = JSON.stringify(`${fixtureRoot}/left/${MISSING_SOURCE}`)
  const destDir = JSON.stringify(`${fixtureRoot}/right`)
  await tauriPage.evaluate(`(async function() {
    await window.__TAURI_INTERNALS__.invoke('copy_between_volumes', {
      sourceVolumeId: 'root',
      sourcePaths: [${src}],
      destVolumeId: 'root',
      destPath: ${destDir},
      config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] }
    });
  })()`)
}

/** Reads the queue window's rows as `{ id, status }[]` from its live DOM. */
async function readRows(queuePage: TauriPage): Promise<{ id: string; status: string }[]> {
  const json = await queuePage.evaluate(`(function() {
    var rows = Array.from(document.querySelectorAll('.queue-row'));
    return JSON.stringify(rows.map(function(r) {
      return { id: r.getAttribute('data-operation-id'), status: r.getAttribute('data-status') };
    }));
  })()`)
  return JSON.parse(json as string) as { id: string; status: string }[]
}

/** The reason text of the first failed row, or `''` when there's no failed row. */
async function readFailureReason(queuePage: TauriPage): Promise<string> {
  const text = await queuePage.evaluate(`(function() {
    var row = document.querySelector('.queue-row[data-status="failed"]');
    if (!row) return '';
    var reason = row.querySelector('.reason-cell');
    return reason ? reason.textContent.replace(/\\s+/g, ' ').trim() : '';
  })()`)
  return text as string
}

/** Opens (or raises) the queue window the way the menu and the palette do. */
async function openQueueWindow(main: TauriPage): Promise<TauriPage> {
  await main.evaluate(`(function() {
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'execute-command', payload: { commandId: 'queue.show' }
    });
  })()`)
  return main.waitForWindow((w) => w.label === QUEUE_LABEL, { timeout: 10000 })
}

async function clickRowButton(queuePage: TauriPage, operationId: string, ariaLabel: string): Promise<void> {
  const idJson = JSON.stringify(operationId)
  const labelJson = JSON.stringify(ariaLabel)
  await queuePage.evaluate(`(function() {
    var row = document.querySelector('.queue-row[data-operation-id=' + JSON.stringify(${idJson}) + ']');
    if (!row) throw new Error('row not found: ' + ${idJson});
    var btn = row.querySelector('[aria-label=' + JSON.stringify(${labelJson}) + ']');
    if (!btn) throw new Error('button not found: ' + ${labelJson});
    btn.click();
  })()`)
}

test.beforeEach(async ({ tauriPage }) => {
  const fixtureRoot = getFixtureRoot()
  recreateFixtures(fixtureRoot)
  // Dedicated multi-file sources, created fresh per test (recreateFixtures wiped
  // left/ except bulk/). See the file header for why a single file won't do.
  makeSourceDir(fixtureRoot, SOURCE_A)
  makeSourceDir(fixtureRoot, SOURCE_B)
  await ensureAppReady(tauriPage)
  // Slow each per-file copy step so the ops stay in flight while we inspect them.
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: ${String(THROTTLE_MS)} })`)
})

// ⚠️ ONE hook, and the order inside it is load-bearing: drain the operations
// FIRST, put the fixture tree back SECOND. Every test here leaves a copy in
// flight (24 files at 250 ms each is ~6 s of work the test never waits out), and
// the tree restore DELETES `left/queue-src-a` + `left/queue-src-b` — they aren't
// in the pristine manifest. Restore an op out from under its own source and the
// copy dies with `SourceNotFound`, which is a RETAINED failure: it raises the
// "no longer exists" toast the leak guard then reports, and its queue row
// outlives the test and fails the NEXT one's "exactly one running row" poll.
// Split across two `test.afterEach` hooks this ordering is invisible (Playwright
// runs same-suite hooks in DECLARATION order), and the spec silently depended on
// the cancel beating the copy's next per-file read by ~30 ms.
test.afterEach(async ({ tauriPage }) => {
  // Clear the throttle FIRST so any in-flight op winds down fast, cancel
  // everything, then WAIT for the operation-manager lane to actually empty.
  // `cancel_operations` returns once cancellation is REQUESTED, not once the ops
  // have wound down; a still-cancelling op leaves the local lane busy, so the
  // next test's foreground F5 copy is admitted as Queued (not Running) and its
  // progress modal never opens — the Linux `operation-queue` flake (rarer on the
  // faster macOS lane). The drain loop runs in the webview so it doesn't depend
  // on `evaluate` returning an async value; Node awaits the IIFE either way.
  // A retained failure never leaves the snapshot on its own (that's the point),
  // so it's dismissed explicitly here — otherwise the drain loop would spin out
  // its whole budget waiting for a row that is designed to stay. The dismiss
  // sits INSIDE the loop, not once before it: an op that dies while the loop is
  // already spinning retains a fresh failure, and a one-shot dismiss ahead of it
  // would miss exactly that one.
  await tauriPage.evaluate(`(async function() {
    try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    for (var i = 0; i < 60; i++) {
      try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
      var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
      if (!remaining || remaining.length === 0) break;
      await new Promise(function(r) { setTimeout(r, 100); });
    }
  })()`)

  // Only now, with nothing reading `left/` any more, put the shared tree back:
  // the post-test leak guard fails whoever leaves it dirty, and the restore is
  // surgical, so it only rewrites what actually drifted.
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Operation queue window', () => {
  test('shows Running + Queued, cancels the queued op, pauses and resumes the running op', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    // The fixture is `TauriPage | BrowserPageAdapter`; the Tauri-only seam here
    // (`waitForWindow`, the helper functions) needs the concrete `TauriPage`.
    // Same cast the other multi-window specs use.
    const main = tauriPage as TauriPage

    // Two same-lane copies: first admits (Running), second queues (Queued).
    await startCopy(main, fixtureRoot, SOURCE_A)
    await startCopy(main, fixtureRoot, SOURCE_B)

    // Open the queue window via the same command the menu / palette use.
    const queuePage = await openQueueWindow(main)

    // One Running + one Queued.
    await expect
      .poll(
        async () => {
          const rows = await readRows(queuePage)
          const statuses = rows.map((r) => r.status).sort()
          return JSON.stringify(statuses)
        },
        { timeout: 15000 },
      )
      .toBe(JSON.stringify(['queued', 'running']))

    // Cancel the queued op: it drops from the list, leaving only the running one.
    const rowsBefore = await readRows(queuePage)
    const queuedId = rowsBefore.find((r) => r.status === 'queued')?.id
    expect(queuedId, 'a queued row exists').toBeTruthy()
    if (!queuedId) throw new Error('no queued row')
    await clickRowButton(queuePage, queuedId, 'Cancel this operation')

    await expect
      .poll(
        async () => {
          const rows = await readRows(queuePage)
          return rows.length
        },
        { timeout: 15000 },
      )
      .toBe(1)

    // The surviving row is the running op. Pause it → status flips to Paused.
    const runningId = (await readRows(queuePage)).find((r) => r.status === 'running')?.id
    expect(runningId, 'a running row exists').toBeTruthy()
    if (!runningId) throw new Error('no running row')
    await clickRowButton(queuePage, runningId, 'Pause this operation')

    await expect
      .poll(
        async () => {
          const rows = await readRows(queuePage)
          return rows.find((r) => r.id === runningId)?.status
        },
        { timeout: 15000 },
      )
      .toBe('paused')

    // Resume it → status flips back to Running.
    await clickRowButton(queuePage, runningId, 'Resume this operation')

    await expect
      .poll(
        async () => {
          const rows = await readRows(queuePage)
          return rows.find((r) => r.id === runningId)?.status
        },
        { timeout: 15000 },
      )
      .toBe('running')
  })

  test('Queue button sends the foreground op to the background; a second same-lane op auto-queues with no second modal', async ({
    tauriPage,
  }) => {
    const fixtureRoot = getFixtureRoot()
    const main = tauriPage as TauriPage

    // Foreground copy via the real F5 flow. Cursor the multi-file source DIR in
    // the left pane (no need to descend into it, which avoids a navigation race),
    // F5, confirm in the destination picker, then the progress dialog opens.
    const found = await moveCursorToFile(main, SOURCE_A)
    expect(found, 'cursor lands on the source dir').toBe(true)
    await main.keyboard.press('F5')
    await main.waitForSelector(TRANSFER_DIALOG, 5000)
    await main.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await main.click(`${TRANSFER_DIALOG} .btn-primary`)

    // The progress modal appears with the background control. This is the first
    // operation, so the queue behind it is empty and the button reads
    // "Background", not "Queue" — the accessible name follows the word.
    await main.waitForSelector(PROGRESS_DIALOG, 5000)
    const BACKGROUND_BUTTON = `${PROGRESS_DIALOG} [aria-label="Keep this running in the background"]`
    await main.waitForSelector(BACKGROUND_BUTTON, 5000)

    // Click it → the modal unmounts and the queue window opens, the op still
    // running in the background.
    await main.click(BACKGROUND_BUTTON)
    await expect.poll(async () => !(await main.isVisible(PROGRESS_DIALOG)), { timeout: 5000 }).toBeTruthy()

    // Sending to the background fires a confirmation toast (the wording is the
    // contract). Assert and dismiss it so the global afterEach leak guard stays
    // clean.
    await expectAndDismissToast(main, 'Still running in the background')

    const queuePage = await main.waitForWindow((w) => w.label === QUEUE_LABEL, { timeout: 10000 })
    await expect
      .poll(
        async () => {
          const rows = await readRows(queuePage)
          return rows.length === 1 && rows[0].status === 'running' ? 'running' : JSON.stringify(rows)
        },
        { timeout: 15000 },
      )
      .toBe('running')

    // Start a SECOND same-lane copy via IPC. Its lane is busy, so the manager
    // admits it as Queued. The queue window shows two rows; no second modal opens
    // in the main window.
    await startCopy(main, fixtureRoot, SOURCE_B)

    await expect
      .poll(
        async () => {
          const rows = await readRows(queuePage)
          return rows
            .map((r) => r.status)
            .sort()
            .join(',')
        },
        { timeout: 15000 },
      )
      .toBe('queued,running')

    // No progress modal stacked in the main window for the queued op.
    expect(await main.isVisible(PROGRESS_DIALOG), 'no second modal for the queued op').toBe(false)
  })

  // The whole reason failures are retained in the backend: the window that would
  // have shown the error is closed at the moment the operation dies.
  test('a failure that happens while the window is closed is still there, with its reason, when it reopens', async ({
    tauriPage,
  }) => {
    const fixtureRoot = getFixtureRoot()
    const main = tauriPage as TauriPage

    // Start a copy that can't succeed, with no queue window open at all.
    await startDoomedCopy(main, fixtureRoot)

    // The main window says so on its own, naming the reason: nothing else would
    // have told the user, since no dialog owns this operation.
    await expectAndDismissToast(main, 'no longer exists')

    // Open the window: the failure is waiting there, with the real reason.
    let queuePage = await openQueueWindow(main)
    await expect
      .poll(async () => (await readRows(queuePage)).map((r) => r.status).join(','), { timeout: 15000 })
      .toBe('failed')
    expect(await readFailureReason(queuePage)).toContain('no longer exists')

    // Close it and let the webview go, taking any frontend-held state with it.
    await closeScopedWindow(main, queuePage, QUEUE_LABEL)

    // Reopen: the row survived the window, because the backend held it.
    queuePage = await openQueueWindow(main)
    await expect
      .poll(async () => (await readRows(queuePage)).map((r) => r.status).join(','), { timeout: 15000 })
      .toBe('failed')
    expect(await readFailureReason(queuePage)).toContain('no longer exists')

    // Dismiss is the only thing that takes it away, and it takes it away for
    // good.
    const failedId = (await readRows(queuePage)).find((r) => r.status === 'failed')?.id
    expect(failedId, 'a failed row exists').toBeTruthy()
    if (!failedId) throw new Error('no failed row')
    await clickRowButton(queuePage, failedId, 'Dismiss this operation')
    await expect.poll(async () => (await readRows(queuePage)).length, { timeout: 10000 }).toBe(0)
  })
})
