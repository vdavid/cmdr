/**
 * E2E tests for the native drag-and-drop ENTRY path across MTP volumes, driven
 * programmatically (real OS drag can't be synthesized in Playwright).
 *
 * Emits the E2E-gated `e2e-trigger-file-drop` event the app forwards to
 * `ExplorerAPI.triggerFileDrop` → the SAME `dragDrop.handleFileDrop` the live
 * drop branch runs, so OUR drop handling (the shared destination guard,
 * source-volume resolution, transfer dialog) is exercised end to end.
 *
 * The cross-volume drops are regression pins:
 *  - dropping onto the read-only SD Card shows the "Read-only device" alert with
 *    the exact copy F5 shows and NO transfer dialog (a drop must hit the same
 *    read-only guard F5 does);
 *  - an MTP→local drop resolves the real MTP source so the counters fill instead
 *    of reading 0 (a wrong source volume id makes the preview report zeros);
 *  - a local→MTP drop resolves the local source for the same reason.
 *
 * Lives on the MTP shard (`mtp-*.spec.ts`); requires `playwright-e2e,virtual-mtp`.
 */

import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { recreateMtpFixtures } from '../e2e-shared/mtp-fixtures.js'
import { initMcpClient, mcpReadResource, mcpSelectVolume, mcpAwaitItem } from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  expectDialogCounters,
  getFixtureRoot,
  isStateClean,
  readDialogCounters,
  triggerFileDrop,
  triggerSelfFileDrop,
  TRANSFER_DIALOG,
} from './helpers.js'

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
type PageLike = TauriPage | BrowserPageAdapter

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'
const SD_CARD = 'Virtual Pixel 9 - SD Card'
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

const ALERT_DIALOG = '[data-dialog-id="alert"]'

/** Discovers the mtp:// path prefix for a named MTP storage from cmdr://state.
 *  The device id is assigned at runtime, so the prefix is derived from the
 *  `id: deviceId:storageId` line (matching the canonical helper in the other
 *  MTP specs), not hardcoded. */
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

/** Reads the registered volume id for a named MTP storage from cmdr://state.
 *  This is what a self-drag records as its source volume id (the dispatchable
 *  backend volume id), distinct from the `mtp://…` path prefix. */
async function getMtpVolumeId(storageName: string): Promise<string> {
  const state = await mcpReadResource('cmdr://state')
  const lines = state.split('\n')
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes(`name: ${storageName}`) && lines[i + 1]?.includes('id:')) {
      return lines[i + 1].trim().replace('id: ', '')
    }
  }
  throw new Error(`MTP volume "${storageName}" not found in cmdr://state`)
}

/** Reads the alert dialog's message text (empty string if not open). */
async function readAlert(tauriPage: PageLike): Promise<{ title: string; message: string }> {
  return tauriPage.evaluate<{ title: string; message: string }>(`(function(){
      var root = document.querySelector('${ALERT_DIALOG}');
      if (!root) return { title: '', message: '' };
      var titleEl = root.querySelector('h2, .modal-title');
      var msgEl = root.querySelector('.message, #alert-dialog-message');
      return {
          title: titleEl ? (titleEl.textContent || '').trim() : '',
          message: msgEl ? (msgEl.textContent || '').trim() : '',
      };
  })()`)
}

/** Dismisses the alert dialog by clicking its button. */
async function dismissAlert(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
      var btn = document.querySelector('${ALERT_DIALOG} button');
      if (btn) btn.click();
  })()`)
  await expect.poll(async () => !(await tauriPage.isVisible(ALERT_DIALOG)), { timeout: 3000 }).toBeTruthy()
}

test.setTimeout(120_000)

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot())
  await initMcpClient(tauriPage)

  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)

  // Reset both panes to the local volume so each test starts from a known place.
  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
          var invoke = window.__TAURI_INTERNALS__.invoke;
          invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
          invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
      })()`)
  }
  await ensureAppReady(tauriPage)
})

test.describe('Programmatic drop entry (MTP)', () => {
  test('dropping onto the read-only SD Card shows the Read-only alert F5 shows, no transfer dialog', async ({
    tauriPage,
  }) => {
    const fixtureRoot = getFixtureRoot()

    // Right pane → the read-only SD Card storage.
    await mcpSelectVolume('right', SD_CARD)
    await mcpAwaitItem('right', 'photos')

    // Drop a local file onto the read-only destination.
    await triggerFileDrop(tauriPage, [path.join(fixtureRoot, 'left', 'file-a.txt')], 'right')

    // The shared destination guard refuses with the exact "Read-only device"
    // alert (the E2E-asserted copy contract) — and NO transfer dialog opens.
    await expect.poll(async () => tauriPage.isVisible(ALERT_DIALOG), { timeout: 5000 }).toBeTruthy()
    const alert = await readAlert(tauriPage)
    expect(alert.title).toBe('Read-only device')
    expect(alert.message).toBe(`"${SD_CARD}" is read-only. You can copy files from it, but not to it.`)
    expect(await tauriPage.isVisible(TRANSFER_DIALOG)).toBe(false)

    await dismissAlert(tauriPage)
  })

  test('an MTP SELF-DRAG onto local builds the transfer from the recorded MTP identity (the live failure)', async ({
    tauriPage,
  }) => {
    // The live bug: dragging from the virtual-MTP pane onto a local pane. The MTP
    // listing's RELATIVE path (`/Documents/report.txt`) lands on the pasteboard
    // and, after wry's drop round-trip, looks exactly like a local absolute path.
    // The resolver can't match it to the MTP volume and falls back to local, so
    // the dialog showed 0 bytes / 0 files. Entering through the self-drag flow
    // (recorded identity = MTP volume id + relative path) is what reality does;
    // the transfer must carry the MTP source so the 50-byte report.txt counts.
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    const mtpVolumeId = await getMtpVolumeId(INTERNAL_STORAGE)

    await triggerSelfFileDrop(
      tauriPage,
      { sourceVolumeId: mtpVolumeId, sourcePaths: ['/Documents/report.txt'] },
      'right',
    )

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    // report.txt is 50 bytes; a self-drag that mis-resolved to local would read 0.
    // Poll the scan to terminal first (the recorded MTP source must let the byte
    // scan stat the right volume).
    await expectDialogCounters(tauriPage, { bytes: '50 bytes', files: 1, dirs: 0 })

    await tauriPage.evaluate(`(function(){
        var ov = document.querySelector('${TRANSFER_DIALOG} .modal-overlay') || document.querySelector('.modal-overlay');
        if (ov) ov.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })()`)
    await expect.poll(async () => !(await tauriPage.isVisible(TRANSFER_DIALOG)), { timeout: 3000 }).toBeTruthy()
  })

  test('an EXTERNAL drop of a full mtp:// path onto local resolves the MTP volume (resolver path)', async ({
    tauriPage,
  }) => {
    // The external-drop-shaped variant: a genuine drop carrying a full absolute
    // mtp:// path (no recorded identity). The resolver matches the MTP root via
    // longest-prefix and the counters fill. Kept alongside the self-drag spec so
    // both entry shapes stay covered.
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    await triggerFileDrop(tauriPage, [`${mtpPath}/Documents/report.txt`], 'right')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await expectDialogCounters(tauriPage, { bytes: '50 bytes', files: 1, dirs: 0 })

    const snapshot = await readDialogCounters(tauriPage)
    expect(snapshot?.files).toBeGreaterThan(0)

    await tauriPage.evaluate(`(function(){
        var ov = document.querySelector('${TRANSFER_DIALOG} .modal-overlay') || document.querySelector('.modal-overlay');
        if (ov) ov.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })()`)
    await expect.poll(async () => !(await tauriPage.isVisible(TRANSFER_DIALOG)), { timeout: 3000 }).toBeTruthy()
  })

  test('dropping a local file onto MTP fills the counters from the resolved volume', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()

    // Right pane → MTP Internal Storage root (the drop destination).
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Drop the local 1 KB file-a.txt onto the MTP pane. The handler resolves the
    // LOCAL source volume so the scan reports the file, not 0.
    await triggerFileDrop(tauriPage, [path.join(fixtureRoot, 'left', 'file-a.txt')], 'right')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await expectDialogCounters(tauriPage, { bytes: '1.00 KB', files: 1, dirs: 0 })

    await tauriPage.evaluate(`(function(){
        var ov = document.querySelector('${TRANSFER_DIALOG} .modal-overlay') || document.querySelector('.modal-overlay');
        if (ov) ov.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })()`)
    await expect.poll(async () => !(await tauriPage.isVisible(TRANSFER_DIALOG)), { timeout: 3000 }).toBeTruthy()
  })
})
