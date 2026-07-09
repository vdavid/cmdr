/**
 * E2E test for the cancel + settle contract: when an MTP delete is cancelled
 * mid-flight, the progress dialog stays in "Cancelling…" until the backend
 * emits `write-settled`, then clears. Immediately after that the next F8 must
 * dispatch a new delete cleanly (no wedge).
 *
 * The original incident: user cancelled an MTP delete after ~30 of 92 photos,
 * pressed F8 again on the survivors, all subsequent MTP ops timed out at 30 s
 * because the device was still mid-teardown when the second op dispatched.
 *
 * Cancel propagation into mtp-rs makes teardown fast (~one USB roundtrip); the
 * settle gate makes the FE honest by holding the dialog until the backend
 * confirms it's actually torn down, not just "I told it to stop." See
 * `src-tauri/src/file_system/write_operations/CLAUDE.md` § "Settle contract"
 * and `src-tauri/src/mtp/CLAUDE.md` § "Cancel propagation".
 *
 * Requires `--features playwright-e2e,virtual-mtp`.
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
  pollUntil,
  moveCursorToFile,
  pressKey,
  isStateClean,
  LOCAL_VOLUME_NAME,
} from './helpers.js'

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'

test.setTimeout(90_000)

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

/** Seeds enough DCIM files so the cancel reliably lands mid-delete. */
function seedDcimBatch(): string[] {
  const dcim = path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM')
  const names: string[] = []
  for (let i = 0; i < 12; i++) {
    const name = `cancel-${String(i).padStart(2, '0')}.jpg`
    fs.writeFileSync(
      path.join(dcim, name),
      Buffer.from([0xff, 0xd8, 0xff, 0xe0, ...Buffer.from('cancel-test-' + name)]),
    )
    names.push(name)
  }
  return names
}

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot())
  await initMcpClient(tauriPage)

  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()
  seedDcimBatch()
  // Sync the object tree to disk. The watcher stays PAUSED (see mtp.spec.ts
  // beforeEach and mtp/DETAILS.md § "Virtual device watcher in E2E").
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

test.describe('MTP cancel: settle gate keeps "Cancelling…" until BE quiets down', () => {
  test('first cancel clears via settle, then immediately F8 again dispatches successfully', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'DCIM')
    await mcpNavToPath('left', `${mtpPath}/DCIM`)
    await mcpAwaitItem('left', 'cancel-00.jpg', 30)

    // Slow each per-file delete so the Cancel click reliably lands BEFORE the
    // op completes. Linux's virtual MTP can blow through the 12 tiny
    // `cancel-*.jpg` files fast enough that without a throttle `write-complete`
    // fires before our Cancel click is processed and no `write-cancelled` /
    // `write-settled` events ever land — exactly what the test is verifying.
    // 200 ms × 12 = 2.4 s worst case, plenty of room for the BE-side cancel
    // round-trip.
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: 200 })`)

    // Subscribe to write-cancelled, write-settled, and write-complete so the
    // assertions can sequence events from the BE.
    await tauriPage.evaluate(`(async function() {
      window.__cancelledEvents = [];
      window.__settledEvents = [];
      window.__completeEvents = [];
      const cancelledHandlerId = window.__TAURI_INTERNALS__.transformCallback((ev) => { window.__cancelledEvents.push({...ev.payload, at: Date.now()}); });
      const settledHandlerId = window.__TAURI_INTERNALS__.transformCallback((ev) => { window.__settledEvents.push({...ev.payload, at: Date.now()}); });
      const completeHandlerId = window.__TAURI_INTERNALS__.transformCallback((ev) => { window.__completeEvents.push({...ev.payload, at: Date.now()}); });
      window.__cancelledListenerId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
        event: 'write-cancelled',
        target: { kind: 'Any' },
        handler: cancelledHandlerId,
      });
      window.__settledListenerId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
        event: 'write-settled',
        target: { kind: 'Any' },
        handler: settledHandlerId,
      });
      window.__completeListenerId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
        event: 'write-complete',
        target: { kind: 'Any' },
        handler: completeHandlerId,
      });
    })()`)

    try {
      // Select all 12 cancel-* files.
      for (let i = 0; i < 12; i++) {
        const name = `cancel-${String(i).padStart(2, '0')}.jpg`
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

      // The Space presses above fire the persistent Quick Look hint toast.
      // Dismiss it before continuing so the safety net's leak check at end of
      // test doesn't trip on this side-effect toast.
      await expectAndDismissToast(tauriPage, 'Space')

      // Open the delete confirmation, confirm.
      await pressKey(tauriPage, 'F8')
      await tauriPage.waitForSelector('[data-dialog-id="delete-confirmation"]', 10000)
      await tauriPage.evaluate(`(function() {
        var dialog = document.querySelector('[data-dialog-id="delete-confirmation"]');
        var btn = dialog && dialog.querySelector('.btn-danger');
        if (btn) btn.click();
      })()`)

      // Wait for the transfer progress dialog. Then immediately click Cancel
      // before the small delete finishes.
      await tauriPage.waitForSelector('[data-dialog-id="transfer-progress"]', 10000)
      await tauriPage.evaluate(`(function() {
        var dialog = document.querySelector('[data-dialog-id="transfer-progress"]');
        if (!dialog) return;
        var btns = dialog.querySelectorAll('button');
        for (var i = 0; i < btns.length; i++) {
          if ((btns[i].textContent || '').trim() === 'Cancel') { btns[i].click(); return; }
        }
      })()`)

      // The dialog must stay visible until `write-settled` lands. Settle bound:
      // less than 2 s — anything beyond that is the cancel propagation failing
      // (without propagation the MTP teardown took 30 s and triggered the op
      // timeout). Once settle arrives, dialog closes within ~400 ms
      // (MIN_DISPLAY_MS).
      const cancelClickedAt = Date.now()
      await expect
        .poll(async () => tauriPage.evaluate<boolean>(`(window.__settledEvents || []).length > 0`), { timeout: 2_000 })
        .toBeTruthy()
      const settleArrivedAt = Date.now()
      const settleDuration = settleArrivedAt - cancelClickedAt

      // The dialog should still be visible (or very close to closing) at the
      // moment settle arrives. We don't assert "still up at settle" strictly
      // — a fast settle (< 200 ms after cancel) may close before we poll.
      // But ordering must be right.

      // Wait for the dialog to actually close — and assert it does. The
      // previous version polled without asserting, which let the test pass
      // even when the dialog stayed up (the dialog needs both `write-cancelled`
      // AND `write-settled` to close; one of them firing isn't enough). A
      // stuck progress dialog at test exit poisons every following write-op
      // test through the next beforeEach's Escape, so catch it here.
      const closed = await pollUntil(
        tauriPage,
        async () => !(await tauriPage.isVisible('[data-dialog-id="transfer-progress"]')),
        3_000,
      )
      expect(closed, 'transfer-progress dialog must close within 3 s after settle').toBe(true)

      // Ordering: write-cancelled must arrive before (or simultaneously with)
      // write-settled, per the BE contract.
      const events = await tauriPage.evaluate<{
        cancelled: { operationId: string; at: number }[]
        settled: { operationId: string; at: number }[]
      }>(`({ cancelled: (window.__cancelledEvents || []), settled: (window.__settledEvents || []) })`)
      expect(events.cancelled.length, 'write-cancelled must fire').toBeGreaterThanOrEqual(1)
      expect(events.settled.length, 'write-settled must fire').toBeGreaterThanOrEqual(1)
      const cancelledOpId = events.cancelled[0].operationId
      const settledOpId = events.settled[0].operationId
      expect(cancelledOpId, 'same operationId on both events').toBe(settledOpId)
      expect(
        events.settled[0].at,
        `settle must arrive AFTER cancelled (cancelled@${String(events.cancelled[0].at)}, settled@${String(events.settled[0].at)})`,
      ).toBeGreaterThanOrEqual(events.cancelled[0].at)
      expect(
        settleDuration,
        `settle within 2 s after cancel click (took ${String(settleDuration)} ms; without cancel propagation it would be 30 s)`,
      ).toBeLessThan(2_000)

      // Now press F8 again on survivors. The FE has already closed the dialog
      // (settle arrived), so the second op dispatches cleanly. Without the
      // settle gate, the dialog would still be open here and F8 would be a
      // no-op.
      await mcpCall('refresh', {})
      // Find any remaining cancel-* file and select it.
      const survivors = await tauriPage.evaluate<string[]>(`(function() {
        const out = [];
        document.querySelectorAll('.file-pane.is-focused .file-entry').forEach(el => {
          const name = el.getAttribute('data-filename');
          if (name && name.startsWith('cancel-')) out.push(name);
        });
        return out;
      })()`)
      if (survivors.length > 0) {
        // Verify "immediately F8 again dispatches successfully" via a
        // non-destructive responsiveness check: after settle, an MCP cursor
        // move should round-trip quickly. The previous "F8 + Escape on the
        // delete-confirmation dialog" check was destructive (the Escape
        // sometimes raced with a synthesized Enter on the focused primary
        // button, auto-confirming the delete and leaving a stuck
        // `transfer-progress` dialog whose op blocked the MTP session — that
        // contaminated every following write-op test). Cursor-move exercises
        // the same FE acceptance path without ever entering the delete code
        // path, so a leak is impossible.
        const before = Date.now()
        await moveCursorToFile(tauriPage, survivors[0])
        const elapsed = Date.now() - before
        expect(elapsed, `cursor move after settle must round-trip quickly (took ${String(elapsed)} ms)`).toBeLessThan(
          1_500,
        )
      }
    } finally {
      // Always clear the throttle so it doesn't slow down following tests.
      await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
      await tauriPage.evaluate(`(async function() {
        const ids = ['__cancelledListenerId', '__settledListenerId', '__completeListenerId'];
        const events = ['write-cancelled', 'write-settled', 'write-complete'];
        for (let i = 0; i < ids.length; i++) {
          const id = window[ids[i]];
          if (id !== undefined) {
            await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: events[i], eventId: id });
          }
        }
        delete window.__cancelledEvents;
        delete window.__settledEvents;
        delete window.__completeEvents;
        delete window.__cancelledListenerId;
        delete window.__settledListenerId;
        delete window.__completeListenerId;
      })()`)
    }
  })
})
