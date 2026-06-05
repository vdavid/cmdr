/**
 * E2E test for the same-volume rename-merge fast path.
 *
 * Moving a folder onto an existing same-named folder on the SAME volume MERGES
 * via server-side renames: the folder itself never prompts, a clashing file
 * INSIDE the merge follows the file policy (prompts under Stop), and a dest-only
 * file survives untouched. Uses MTP→MTP so it exercises the real
 * `move_within_same_volume` rename-merge path (a same-`root` move would route
 * through the local-FS engine instead).
 *
 * Requires the app to be built with `--features playwright-e2e,virtual-mtp`.
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { recreateMtpFixtures, writeMtpDrainSentinel, MTP_FIXTURE_ROOT } from '../e2e-shared/mtp-fixtures.js'
import {
  initMcpClient,
  mcpCall,
  mcpReadResource,
  mcpSelectVolume,
  mcpNavToPath,
  mcpAwaitItem,
} from '../e2e-shared/mcp-client.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  focusPane,
  getFixtureRoot,
  isStateClean,
  TRANSFER_DIALOG,
} from './helpers.js'
import {
  waitForConflictPolicy,
  selectConflictPolicy,
  clickTransferStart,
  waitForDialogsToClose,
  waitForConflict,
  resolveConflict,
} from './conflict-helpers.js'

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

const INTERNAL = path.join(MTP_FIXTURE_ROOT, 'internal')

/** Discovers the mtp:// path prefix for a named MTP storage from cmdr://state. */
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

async function bothPanesOnLocalVolume(): Promise<boolean> {
  const state = await mcpReadResource('cmdr://state')
  const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) => line.replace(/^\n {2}volume: /, ''))
  return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
}

test.setTimeout(120_000)

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot())
  await initMcpClient(tauriPage)

  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()

  // Build the merge fixture INSIDE the MTP backing dir, then drain + rescan.
  //  - Documents/album: the SOURCE folder (a fresh file + a clashing file).
  //  - album (at root):  the DEST folder (a clashing file + a dest-only keeper).
  fs.mkdirSync(path.join(INTERNAL, 'Documents', 'album'), { recursive: true })
  fs.writeFileSync(path.join(INTERNAL, 'Documents', 'album', 'fresh.txt'), 'SRC-fresh')
  fs.writeFileSync(path.join(INTERNAL, 'Documents', 'album', 'clash.txt'), 'SRC-clash')
  fs.mkdirSync(path.join(INTERNAL, 'album'), { recursive: true })
  fs.writeFileSync(path.join(INTERNAL, 'album', 'clash.txt'), 'DEST-clash')
  fs.writeFileSync(path.join(INTERNAL, 'album', 'keep.txt'), 'DEST-keep')

  const sentinel = writeMtpDrainSentinel()
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('resync_virtual_mtp_after_disk_change', { sentinelSuffix: ${JSON.stringify(sentinel)} })`,
  )

  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
      var invoke = window.__TAURI_INTERNALS__.invoke;
      invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
      invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
    })()`)
    await expect.poll(() => bothPanesOnLocalVolume(), { timeout: 5000 }).toBeTruthy()
  }
})

test('same-volume MTP folder move auto-merges; file clash inside prompts; dest-only survives', async ({
  tauriPage,
}) => {
  await ensureAppReady(tauriPage)
  const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

  // Left pane: MTP Documents (holds the source `album`).
  await mcpSelectVolume('left', INTERNAL_STORAGE)
  await mcpAwaitItem('left', 'Documents')
  await mcpNavToPath('left', `${mtpPath}/Documents`)
  await mcpAwaitItem('left', 'album')

  // Right pane: MTP root (holds the dest `album` to merge into).
  await mcpSelectVolume('right', INTERNAL_STORAGE)
  await mcpAwaitItem('right', 'album')

  await focusPane(tauriPage, 0)
  await mcpCall('move_cursor', { pane: 'left', filename: 'album' })
  await dispatchMenuCommand(tauriPage, 'file.move')

  // The dialog opens with the default Stop policy. The folder collision is NOT a
  // conflict — it surfaces as an informational "folders will merge" line, not a
  // folder prompt. The file-policy radios still show (a merge can surface file
  // clashes), so leave the policy on Stop ("Ask for each") and start.
  await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
  await waitForConflictPolicy(tauriPage)
  await selectConflictPolicy(tauriPage, 'stop')
  await clickTransferStart(tauriPage)

  // The folder merged silently; the FIRST (and only) prompt is the deep file
  // clash on `clash.txt`. Answer Overwrite.
  const conflict = await waitForConflict(tauriPage, 30000)
  expect(conflict.filename).toContain('clash.txt')
  expect(conflict.isFileOverFolder, 'a file clash is not the file-over-folder variant').toBe(false)
  await resolveConflict(tauriPage, 'Overwrite')

  await waitForDialogsToClose(tauriPage, 30000)

  // Settle on disk: the source `Documents/album` is gone, the dest merged.
  const destAlbum = path.join(INTERNAL, 'album')
  const srcAlbum = path.join(INTERNAL, 'Documents', 'album')
  await expect
    .poll(
      () => {
        if (fs.existsSync(srcAlbum)) return false
        if (!fs.existsSync(path.join(destAlbum, 'fresh.txt'))) return false
        return fs.readFileSync(path.join(destAlbum, 'clash.txt'), 'utf-8') === 'SRC-clash'
      },
      { timeout: 20000 },
    )
    .toBeTruthy()

  // Folder merged: the source-only file arrived.
  expect(fs.readFileSync(path.join(destAlbum, 'fresh.txt'), 'utf-8')).toBe('SRC-fresh')
  // The clashing file was Overwritten with the source bytes.
  expect(fs.readFileSync(path.join(destAlbum, 'clash.txt'), 'utf-8')).toBe('SRC-clash')
  // THE INVARIANT: the dest-only file survives untouched.
  expect(fs.readFileSync(path.join(destAlbum, 'keep.txt'), 'utf-8')).toBe('DEST-keep')
  // The fully-moved source folder is gone (its spine was deleted inside-out).
  expect(fs.existsSync(srcAlbum)).toBe(false)

  await mcpCall('refresh', {})
  await expectAndDismissToast(tauriPage, 'Move complete', { timeout: 30000 })
})
