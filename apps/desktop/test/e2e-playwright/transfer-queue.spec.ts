/**
 * E2E for the transfer-queue window.
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
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, expectAndDismissToast, getFixtureRoot, moveCursorToFile, TRANSFER_DIALOG } from './helpers.js'
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

test.afterEach(async ({ tauriPage }) => {
  // Cancel anything still in flight and clear the throttle so a leaked op can't
  // bleed into the next test.
  await tauriPage.evaluate(`(async function() {
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
  })()`)
})

test.describe('Transfer queue window', () => {
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
    await main.evaluate(`(function() {
      window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'execute-command', payload: { commandId: 'queue.show' }
      });
    })()`)
    const queuePage = await main.waitForWindow((w) => w.label === QUEUE_LABEL, { timeout: 10000 })

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
    await clickRowButton(queuePage, queuedId, 'Cancel this transfer')

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
    await clickRowButton(queuePage, runningId, 'Pause this transfer')

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
    await clickRowButton(queuePage, runningId, 'Resume this transfer')

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

    // The progress modal appears with the Queue control.
    await main.waitForSelector(PROGRESS_DIALOG, 5000)
    await main.waitForSelector(`${PROGRESS_DIALOG} [aria-label="Send to the transfer queue"]`, 5000)

    // Click Queue → the modal unmounts and the queue window opens, the op still
    // running in the background.
    await main.click(`${PROGRESS_DIALOG} [aria-label="Send to the transfer queue"]`)
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
})
