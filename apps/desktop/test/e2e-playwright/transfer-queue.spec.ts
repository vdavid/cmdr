/**
 * E2E for the transfer-queue window (M3 of the transfer-queue + pause feature).
 *
 * Two same-lane local copies serialize behind the operation manager (lane budget
 * 1 per device, and both copies touch the local volume's lane): the first runs,
 * the second queues. The queue window then shows one Running + one Queued row.
 * From there we cancel the queued one (it drops) and pause + resume the running
 * one (its status flips to Paused, then back to Running).
 *
 * The copies are kicked off directly through the `copy_between_volumes` IPC (the
 * same command the F5 dialog calls), which registers the op and returns its id
 * immediately — no modal needed. A test throttle keeps each copy alive long
 * enough to observe the Running/Queued/Paused states.
 *
 * Requires `--features playwright-e2e`.
 */

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, getFixtureRoot } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const QUEUE_LABEL = 'queue'

test.setTimeout(90_000)

/** Starts a local→local copy of one bulk file into `right/` via the production
 *  IPC. Returns nothing; the op registers and the manager admits or queues it. */
async function startCopy(tauriPage: TauriPage, fixtureRoot: string, name: string, destName: string): Promise<void> {
  const src = JSON.stringify(`${fixtureRoot}/left/bulk/${name}`)
  const destDir = JSON.stringify(`${fixtureRoot}/right`)
  // `copy_between_volumes` args (camelCase): sourceVolumeId, sourcePaths,
  // destVolumeId, destPath, config. Both volumes are the default local "root",
  // so the two copies share the local lane and serialize.
  void destName
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
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(tauriPage)
  // Slow each copy loop tick so the ops stay in flight while we inspect them.
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: 200 })`)
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

    // Two same-lane copies: first admits (Running), second queues (Queued).
    await startCopy(tauriPage, fixtureRoot, 'large-1.dat', 'large-1.dat')
    await startCopy(tauriPage, fixtureRoot, 'large-2.dat', 'large-2.dat')

    // Open the queue window via the same command the menu / palette use.
    await tauriPage.evaluate(`(function() {
      window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'execute-command', payload: { commandId: 'queue.show' }
      });
    })()`)
    const queuePage = await tauriPage.waitForWindow((w) => w.label === QUEUE_LABEL, { timeout: 10000 })

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
    await clickRowButton(queuePage, queuedId!, 'Cancel this transfer')

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
    await clickRowButton(queuePage, runningId!, 'Pause this transfer')

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
    await clickRowButton(queuePage, runningId!, 'Resume this transfer')

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
})
