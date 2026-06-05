/**
 * E2E tests for conflict resolution during MTP file operations.
 *
 * Verifies that move operations (F6) involving MTP volumes properly detect
 * destination conflicts and apply the chosen resolution policy (overwrite/skip).
 * Covers both cross-volume (MTP↔local) and same-volume (MTP→MTP) moves.
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
  CTRL_OR_META,
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
} from './conflict-helpers.js'

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

/** True when both panes show the local volume in cmdr://state. */
async function bothPanesOnLocalVolume(): Promise<boolean> {
  const state = await mcpReadResource('cmdr://state')
  const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) => line.replace(/^\n {2}volume: /, ''))
  return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
}

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

// MTP protocol overhead requires longer timeouts
test.setTimeout(120_000)

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot())
  await initMcpClient(tauriPage)

  // Pause watcher → recreate fixtures → settle + rescan + resume (atomic).
  // The combined IPC drains late FSEvents while still paused; see
  // `resync_virtual_mtp_after_disk_change` in commands/mtp.rs.
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()
  const sentinel = writeMtpDrainSentinel()
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('resync_virtual_mtp_after_disk_change', { sentinelSuffix: ${JSON.stringify(sentinel)} })`,
  )

  // Reset both panes to local volume; short-circuit when already clean.
  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
      var invoke = window.__TAURI_INTERNALS__.invoke;
      invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
      invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
    })()`)
    await expect.poll(() => bothPanesOnLocalVolume(), { timeout: 5000 }).toBeTruthy()
    // Previously: double-Escape + best-effort modal-overlay poll to clean up
    // dialogs leaked from prior tests. The global afterEach safety net in
    // fixtures.ts now catches and auto-cleans any leaks at the point of leak,
    // so this defensive cleanup is no longer needed.
  }
})

// ── Cross-volume move conflicts (MTP ↔ local) ──────────────────────────────

test.describe('MTP cross-volume move conflicts', () => {
  test('MTP-to-local move with overwrite replaces dest and removes source', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()

    // Create conflicting file in local right/
    fs.writeFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'local-version')

    // Left pane: MTP Documents (has report.txt from fixtures)
    await ensureAppReady(tauriPage)
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Move report.txt to local right/ (which already has report.txt)
    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    // Wait for the MTP operation to settle on disk: dest contains MTP content AND source removed.
    const destPath = path.join(fixtureRoot, 'right', 'report.txt')
    const srcPath = path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')
    await expect
      .poll(
        () => {
          if (fs.existsSync(srcPath)) return false
          if (!fs.existsSync(destPath)) return false
          return fs.readFileSync(destPath, 'utf-8').includes('Quarterly report')
        },
        { timeout: 15000 },
      )
      .toBeTruthy()

    // Dest should have MTP content (overwritten)
    const destContent = fs.readFileSync(destPath, 'utf-8')
    expect(destContent).toContain('Quarterly report')

    // Source should be deleted from MTP
    expect(fs.existsSync(srcPath)).toBe(false)

    // Transfer fires a "Moved 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.', { timeout: 30000 })
  })

  test('MTP-to-local move with skip preserves both files', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()

    // Create conflicting file in local right/
    fs.writeFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'local-version')

    await ensureAppReady(tauriPage)
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    // Dest unchanged (skip)
    const destContent = fs.readFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'utf-8')
    expect(destContent).toBe('local-version')

    // Source still exists (not moved because skipped)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(true)

    // Transfer fires a "Move complete" toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Move complete', { timeout: 30000 })
  })

  test('local-to-MTP move with overwrite replaces MTP file', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()

    // Create conflicting file in MTP root
    fs.writeFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt'), 'mtp-version')
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)

    // Left pane: local left/ (has file-a.txt from local fixtures)
    // Right pane: MTP Internal Storage root
    await ensureAppReady(tauriPage)
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'file-a.txt', 15)

    await mcpCall('move_cursor', { pane: 'left', filename: 'file-a.txt' })
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    // Wait for the move to settle on disk: MTP file has overwritten content AND source removed.
    const mtpDest = path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt')
    const localSrc = path.join(fixtureRoot, 'left', 'file-a.txt')
    const expectedContent = 'A'.repeat(1024)
    await expect
      .poll(
        () => {
          if (fs.existsSync(localSrc)) return false
          if (!fs.existsSync(mtpDest)) return false
          return fs.readFileSync(mtpDest, 'utf-8') === expectedContent
        },
        { timeout: 15000 },
      )
      .toBeTruthy()
    await mcpCall('refresh', {})

    // MTP file should have local content (overwritten); local fixture is 1024 'A' chars
    const mtpContent = fs.readFileSync(mtpDest, 'utf-8')
    expect(mtpContent).toBe(expectedContent)

    // Local source should be gone (moved)
    expect(fs.existsSync(localSrc)).toBe(false)

    // Transfer fires a "Moved 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.', { timeout: 30000 })
  })
})

// ── Same-volume move conflicts (within MTP) ─────────────────────────────────

test.describe('MTP same-volume move conflicts', () => {
  test('same-volume MTP move with overwrite replaces dest', async ({ tauriPage }) => {
    // Create a file at MTP root that conflicts with Documents/report.txt
    fs.writeFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'report.txt'), 'root-version')
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)

    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Left pane: MTP Documents (has report.txt)
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Right pane: MTP root (also has report.txt)
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'report.txt')

    await focusPane(tauriPage, 0)

    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    // Wait for the same-volume MTP move to settle on disk.
    const rootPath = path.join(MTP_FIXTURE_ROOT, 'internal', 'report.txt')
    const docsSrc = path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')
    await expect
      .poll(
        () => {
          if (fs.existsSync(docsSrc)) return false
          if (!fs.existsSync(rootPath)) return false
          return fs.readFileSync(rootPath, 'utf-8').includes('Quarterly report')
        },
        { timeout: 15000 },
      )
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Root report.txt should have Documents content (overwritten)
    const rootContent = fs.readFileSync(rootPath, 'utf-8')
    expect(rootContent).toContain('Quarterly report')

    // Source should be gone from Documents
    expect(fs.existsSync(docsSrc)).toBe(false)

    // Transfer fires a "Moved 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.', { timeout: 30000 })
  })

  test('same-volume MTP move with skip preserves both files', async ({ tauriPage }) => {
    // Create a file at MTP root that conflicts with Documents/report.txt
    fs.writeFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'report.txt'), 'root-version')
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)

    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'report.txt')

    await focusPane(tauriPage, 0)

    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await dispatchMenuCommand(tauriPage, 'file.move')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    // Root file unchanged (skip)
    const rootContent = fs.readFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'report.txt'), 'utf-8')
    expect(rootContent).toBe('root-version')

    // Source still exists (not moved because skipped)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(true)

    // Transfer fires a "Move complete" toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Move complete', { timeout: 30000 })
  })
})

// ── Cross-volume COPY conflicts (MTP -> local) ─────────────────────────────

test.describe('MTP cross-volume copy conflicts', () => {
  test('MTP-to-local copy with apply-to-all Skip credits byte progress', async ({ tauriPage }) => {
    // Regression test for: per-iter Skip in the async transfer driver bumps
    // `filesDone` but not `bytesDone`. User-visible symptom: the file counter
    // moves forward while the size counter stays at 0 % on Skip-All copies
    // where conflicts resolve via `apply_to_all` (the mid-operation conflict
    // dialog), not via the FE's pre-flight `preKnownConflicts` bulk-skip.
    //
    // Setup: MTP `Documents/` has `report.txt` + `notes.txt`. Pre-populate
    // matching files at local `right/` so both source files conflict at dest.
    // Trigger MTP -> local copy with the DEFAULT (`stop`) conflict policy so
    // `preKnownConflicts` stays empty. Intercept the first `write-conflict`
    // event and resolve it via `resolve_write_conflict(opId, Skip, true)` so
    // `apply_to_all` latches and subsequent conflicts auto-resolve via the
    // driver's per-iter Skip arm. Assert the final `write-progress` event has
    // both axes landed at total.
    const fixtureRoot = getFixtureRoot()
    fs.writeFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'local-report')
    fs.writeFileSync(path.join(fixtureRoot, 'right', 'notes.txt'), 'local-notes')

    await ensureAppReady(tauriPage)
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')
    await mcpAwaitItem('left', 'notes.txt')

    // Subscribe to write-conflict (for the apply-to-all answer) and
    // write-complete (carries the authoritative `bytes_processed` /
    // `files_processed`) BEFORE triggering the copy. write-progress is
    // throttled to 200 ms and 2 fast in-memory Skips don't reliably emit a
    // post-iter event — write-complete is the source of truth.
    await tauriPage.evaluate(`(async function() {
        window.__skipBytesTestConflicts = [];
        window.__skipBytesTestComplete = null;
        const conflictHandler = (event) => { window.__skipBytesTestConflicts.push(event.payload); };
        const completeHandler = (event) => { window.__skipBytesTestComplete = event.payload; };
        const conflictHandlerId = window.__TAURI_INTERNALS__.transformCallback(conflictHandler);
        const completeHandlerId = window.__TAURI_INTERNALS__.transformCallback(completeHandler);
        window.__skipBytesTestConflictId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
          event: 'write-conflict',
          target: { kind: 'Any' },
          handler: conflictHandlerId,
        });
        window.__skipBytesTestCompleteId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
          event: 'write-complete',
          target: { kind: 'Any' },
          handler: completeHandlerId,
        });
      })()`)

    try {
      // Select both files in the MTP pane. Meta is Cmd on macOS but Super on
      // Linux where select-all is bound to Ctrl+A — use the platform-aware
      // modifier rather than a hardcoded `Meta`.
      await tauriPage.keyboard.down(CTRL_OR_META)
      await tauriPage.keyboard.press('A')
      await tauriPage.keyboard.up(CTRL_OR_META)
      await dispatchMenuCommand(tauriPage, 'file.copy')

      await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
      await waitForConflictPolicy(tauriPage)
      // Leave the dropdown at its default ('stop') so preKnownConflicts stays
      // empty and conflicts surface per-iter via write-conflict.
      await clickTransferStart(tauriPage)

      // Wait for the first write-conflict event, then resolve via IPC.
      await expect
        .poll(async () => tauriPage.evaluate<boolean>(`(window.__skipBytesTestConflicts ?? []).length > 0`), {
          timeout: 10000,
          intervals: [50],
        })
        .toBeTruthy()
      const firstConflict = await tauriPage.evaluate<{ operationId: string }>(`window.__skipBytesTestConflicts[0]`)
      expect(firstConflict.operationId).toBeTruthy()
      await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('resolve_write_conflict', {
          operationId: '${firstConflict.operationId}',
          resolution: 'skip',
          applyToAll: true,
        })`)

      await waitForDialogsToClose(tauriPage, 30000)

      // Dest content unchanged (both files were skipped).
      expect(fs.readFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'utf-8')).toBe('local-report')
      expect(fs.readFileSync(path.join(fixtureRoot, 'right', 'notes.txt'), 'utf-8')).toBe('local-notes')

      // Source files still on MTP (skip is non-destructive for copy).
      expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(true)
      expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(true)

      // write-complete carries the authoritative final tallies. With the bug,
      // every per-iter Skip bumps `filesProcessed` by 1 but `bytesProcessed`
      // stays at 0, so the dialog's size bar reads 0 % at the end.
      const completed = await tauriPage.evaluate<{
        filesProcessed: number
        bytesProcessed: number
      } | null>(`window.__skipBytesTestComplete`)
      const expectedBytes =
        fs.statSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')).size +
        fs.statSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt')).size

      expect(completed).not.toBeNull()
      expect(completed?.filesProcessed).toBe(2)
      expect(completed?.bytesProcessed).toBe(expectedBytes)
    } finally {
      await tauriPage.evaluate(`(async function() {
          const conflictId = window.__skipBytesTestConflictId;
          const completeId = window.__skipBytesTestCompleteId;
          if (conflictId !== undefined) {
            await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: 'write-conflict', eventId: conflictId });
          }
          if (completeId !== undefined) {
            await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: 'write-complete', eventId: completeId });
          }
          delete window.__skipBytesTestConflicts;
          delete window.__skipBytesTestComplete;
          delete window.__skipBytesTestConflictId;
          delete window.__skipBytesTestCompleteId;
        })()`)
    }

    // Transfer fires a "Copy complete" toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Copy complete', { timeout: 30000 })
  })
})
