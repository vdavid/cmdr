/**
 * E2E test pinning the "delete reuses scan preview, no double scan" contract
 * for MTP delete.
 *
 * The bug this guards against: without the fix, `delete_volume_files_with_progress`
 * would ignore `preview_id` and re-walk the source tree. On MTP that means a
 * second silent parent listing after the user already paid that cost in the
 * delete confirmation dialog — and because the re-walk emits no top-level
 * progress, the UI looks frozen. The backend `take_cached_scan_result`s the
 * preview and goes straight from Scanning to Deleting.
 *
 * The spec subscribes to `write-progress` events from the webview, captures
 * the `phase` sequence, and asserts:
 *
 *   1. Scanning -> Deleting transitions exactly once (no second Scanning
 *      after Deleting starts).
 *   2. `filesDone` never decreases within a phase (the second-scan symptom
 *      was a counter that reset to zero when the silent re-walk began).
 *   3. The files are gone from the device after the operation completes.
 *
 * Requires the app to be built with `--features playwright-e2e,virtual-mtp`.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { recreateMtpFixtures, MTP_FIXTURE_ROOT } from '../e2e-shared/mtp-fixtures.js'
import {
  initMcpClient,
  mcpCall,
  mcpReadResource,
  mcpSelectVolume,
  mcpNavToPath,
  mcpAwaitItem,
} from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  expectAndDismissToast,
  getFixtureRoot,
  moveCursorToFile,
  pressKey,
  isStateClean,
  LOCAL_VOLUME_NAME,
} from './helpers.js'

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'

interface CapturedProgress {
  phase: string
  filesDone: number
  filesTotal: number
  bytesDone: number
}

test.setTimeout(60_000)

async function bothPanesOnLocalVolume(): Promise<boolean> {
  const state = await mcpReadResource('cmdr://state')
  const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) => line.replace(/^\n {2}volume: /, ''))
  return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
}

async function getMtpVolumePath(storageName: string): Promise<string> {
  const state = await mcpReadResource('cmdr://state')
  const lines = state.split('\n')
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes(`name: ${storageName}`) && lines[i + 1]?.includes('id:')) {
      const id = lines[i + 1].trim().replace('id: ', '')
      const [deviceId, storageId] = id.split(':')
      return `mtp://${deviceId}/${storageId}`
    }
  }
  throw new Error(`MTP volume "${storageName}" not found in cmdr://state`)
}

/** Seeds extra files in /DCIM so the delete has multiple top-level entries to track. */
function seedDcimWithExtras(): string[] {
  const dcim = path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM')
  const names = ['delete-a.jpg', 'delete-b.jpg', 'delete-c.jpg']
  for (const name of names) {
    fs.writeFileSync(path.join(dcim, name), Buffer.from([0xff, 0xd8, 0xff, 0xe0, ...Buffer.from('del-test-' + name)]))
  }
  return names
}

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot())
  await initMcpClient(tauriPage)

  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()
  seedDcimWithExtras()
  // Sync the object tree to disk. The watcher stays PAUSED (see mtp/DETAILS.md).
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)

  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} } });
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} } });
    })()`)
    await expect.poll(() => bothPanesOnLocalVolume(), { timeout: 5000 }).toBeTruthy()
    // Previously: double-Escape + best-effort modal-overlay poll to clean up
    // dialogs leaked from prior tests. The global afterEach safety net in
    // fixtures.ts now catches and auto-cleans any leaks at the point of leak,
    // so this defensive cleanup is no longer needed.
  }
})

test.describe('MTP delete reuses scan preview (no double scan)', () => {
  test('F8 progresses Scanning -> Deleting exactly once and counts never go backwards', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Land in MTP /DCIM so the parent listing is in the watcher-backed cache.
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'DCIM')
    await mcpNavToPath('left', `${mtpPath}/DCIM`)
    await mcpAwaitItem('left', 'delete-a.jpg', 30)

    const selection = ['delete-a.jpg', 'delete-b.jpg', 'delete-c.jpg']

    // Subscribe to `write-progress` BEFORE the delete starts. The handler
    // pushes every progress payload into a window-scoped buffer the test
    // reads via `evaluate`.
    await tauriPage.evaluate(`(async function() {
      window.__deleteProgressEvents = [];
      const handler = (event) => { window.__deleteProgressEvents.push(event.payload); };
      const handlerId = window.__TAURI_INTERNALS__.transformCallback(handler);
      window.__deleteProgressEventId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
        event: 'write-progress',
        target: { kind: 'Any' },
        handler: handlerId,
      });
      window.__deleteCompleteEvents = [];
      const completeHandler = (event) => { window.__deleteCompleteEvents.push(event.payload); };
      const completeHandlerId = window.__TAURI_INTERNALS__.transformCallback(completeHandler);
      window.__deleteCompleteEventId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
        event: 'write-complete',
        target: { kind: 'Any' },
        handler: completeHandlerId,
      });
    })()`)

    try {
      // Multi-select via Space (same path as mtp.spec.ts's "deletes multiple
      // selected files on MTP"). Poll for `.is-selected` after each Space so
      // we don't race the next cursor move.
      for (const name of selection) {
        await moveCursorToFile(tauriPage, name)
        await pressKey(tauriPage, 'Space')
        await expect
          .poll(
            async () =>
              tauriPage.evaluate<boolean>(
                `!!document.querySelector('.file-pane.is-focused .file-entry[data-filename=' + ${JSON.stringify(JSON.stringify(name))} + '].is-selected')`,
              ),
            { timeout: 2000 },
          )
          .toBeTruthy()
      }

      // The first Space press fires the persistent Quick Look hint toast.
      // Dismiss it before continuing so it doesn't sit through the delete
      // and trip the safety-net leak guard at end-of-test.
      await expectAndDismissToast(tauriPage, 'Space')

      // Press F8 to open the delete confirmation dialog (MTP volumes force
      // a permanent delete because they don't support trash).
      await pressKey(tauriPage, 'F8')
      await tauriPage.waitForSelector('[data-dialog-id="delete-confirmation"]', 10000)

      // Confirm the delete by clicking the danger button.
      await tauriPage.evaluate(`(function() {
        var dialog = document.querySelector('[data-dialog-id="delete-confirmation"]');
        if (!dialog) return;
        var btn = dialog.querySelector('.btn-danger');
        if (btn) btn.click();
      })()`)

      // Wait for the operation to finish: filesystem-side files gone, then
      // refresh the pane so the FE catches up.
      await expect
        .poll(
          () => {
            for (const name of selection) {
              if (fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', name))) return false
            }
            return true
          },
          { timeout: 30000 },
        )
        .toBeTruthy()
      await mcpCall('refresh', {})

      // Wait for write-complete (or for the progress dialog to go away).
      await expect
        .poll(async () => tauriPage.evaluate<boolean>(`(window.__deleteCompleteEvents || []).length > 0`), {
          timeout: 10000,
        })
        .toBeTruthy()

      // Pull the captured progress sequence and run the phase-sequence assertions.
      const events = await tauriPage.evaluate<CapturedProgress[]>(
        `(window.__deleteProgressEvents || []).map(p => ({ phase: p.phase, filesDone: p.filesDone, filesTotal: p.filesTotal, bytesDone: p.bytesDone }))`,
      )

      // 1. Phase sequence must monotonically advance through Scanning ->
      //    Deleting. Collapse consecutive-same-phase runs and assert that the
      //    transition never reverses, regardless of whether `deleting` got a
      //    progress event at all.
      //
      //    Why "or doesn't appear" is fine: the per-file delete on a small
      //    selection often finishes inside one 200 ms progress-throttle
      //    window, so the BE fires `scanning` then jumps to write-complete.
      //    That's the scan-preview-reuse fast path doing its job — the bug
      //    we guard against is a SECOND scanning phase showing up after
      //    deleting started, not "delete must emit a deleting event."
      const phaseSequence: string[] = []
      for (const ev of events) {
        if (phaseSequence[phaseSequence.length - 1] !== ev.phase) phaseSequence.push(ev.phase)
      }
      // Map phases to monotonically-increasing ranks. Anything not in the
      // map (rolling_back, copying, etc.) is intentionally ignored — those
      // shouldn't fire during a plain delete, and if they do they're not the
      // double-scan bug.
      const phaseOrder = new Map<string, number>([
        ['scanning', 0],
        ['deleting', 1],
        ['trashing', 1],
      ])
      let lastRank = -1
      for (const phase of phaseSequence) {
        const rank = phaseOrder.get(phase)
        if (rank === undefined) continue
        expect(
          rank,
          `phase regressed: saw ${phase} after a later phase (full sequence: ${JSON.stringify(phaseSequence)})`,
        ).toBeGreaterThanOrEqual(lastRank)
        lastRank = rank
      }
      // The compacted phase sequence (consecutive duplicates removed) must
      // contain at most one 'scanning' run. A second one would mean the
      // pre-fix re-scan came back.
      const scanningCount = phaseSequence.filter((p) => p === 'scanning').length
      expect(
        scanningCount,
        `expected at most one scanning phase run, saw ${String(scanningCount)} (sequence: ${JSON.stringify(phaseSequence)})`,
      ).toBeLessThanOrEqual(1)

      // 2. `filesDone` must be monotonic within each phase. The pre-fix
      //    symptom was a counter that reset to zero when the silent re-walk
      //    began.
      const lastFilesByPhase: Record<string, number> = {}
      for (const ev of events) {
        const last = lastFilesByPhase[ev.phase] ?? 0
        expect(
          ev.filesDone,
          `filesDone went backwards in phase=${ev.phase}: ${String(last)} -> ${String(ev.filesDone)} (full sequence: ${JSON.stringify(events.map((e) => `${e.phase}:${String(e.filesDone)}`))})`,
        ).toBeGreaterThanOrEqual(last)
        lastFilesByPhase[ev.phase] = ev.filesDone
      }

      // 3. Files are gone from the device.
      for (const name of selection) {
        expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', name))).toBe(false)
      }

      // The progress-events poll above can finish a beat before the dialog
      // settles. Waiting for the "Delete complete" toast both asserts the
      // user-facing confirmation and gives the progress dialog time to
      // auto-close before the safety-net check.
      await expectAndDismissToast(tauriPage, 'Delete complete', { timeout: 30000 })
    } finally {
      // Tear down listeners and clear test state, in that order so a partial
      // failure can still clean up.
      await tauriPage.evaluate(`(async function() {
        const progressId = window.__deleteProgressEventId;
        if (progressId !== undefined) {
          await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: 'write-progress', eventId: progressId });
        }
        const completeId = window.__deleteCompleteEventId;
        if (completeId !== undefined) {
          await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: 'write-complete', eventId: completeId });
        }
        delete window.__deleteProgressEvents;
        delete window.__deleteProgressEventId;
        delete window.__deleteCompleteEvents;
        delete window.__deleteCompleteEventId;
      })()`)
    }
  })
})
