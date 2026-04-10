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
import { recreateMtpFixtures, MTP_FIXTURE_ROOT } from '../e2e-shared/mtp-fixtures.js'
import {
  initMcpClient,
  mcpCall,
  mcpReadResource,
  mcpSelectVolume,
  mcpNavToPath,
  mcpAwaitItem,
  mcpSwitchPane,
} from '../e2e-shared/mcp-client.js'
import { ensureAppReady, getFixtureRoot, sleep, TRANSFER_DIALOG } from './helpers.js'
import {
  waitForConflictPolicy,
  selectConflictPolicy,
  clickTransferStart,
  waitForDialogsToClose,
} from './conflict-helpers.js'

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

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

  // Pause watcher → recreate fixtures → rescan → resume to prevent stale events
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('resume_virtual_mtp_watcher')`)

  // Reset both panes to local volume
  await tauriPage.evaluate(`(function() {
    var invoke = window.__TAURI_INTERNALS__.invoke;
    invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
    invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
  })()`)
  await sleep(2000)
  await tauriPage.keyboard.press('Escape')
  await sleep(200)
  await tauriPage.keyboard.press('Escape')
  await sleep(200)
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
    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    // Wait for MTP operation
    await sleep(3000)

    // Dest should have MTP content (overwritten)
    const destContent = fs.readFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'utf-8')
    expect(destContent).toContain('Quarterly report')

    // Source should be deleted from MTP
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
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
    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    await sleep(3000)

    // Dest unchanged (skip)
    const destContent = fs.readFileSync(path.join(fixtureRoot, 'right', 'report.txt'), 'utf-8')
    expect(destContent).toBe('local-version')

    // Source still exists (not moved because skipped)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(true)
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
    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    await sleep(3000)
    await mcpCall('refresh', {})

    // MTP file should have local content (overwritten) — local fixture is 1024 'A' chars
    const mtpContent = fs.readFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt'), 'utf-8')
    expect(mtpContent).toBe('A'.repeat(1024))

    // Local source should be gone (moved)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(false)
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

    // Ensure left pane is focused
    await mcpSwitchPane()
    await sleep(200)
    await mcpSwitchPane()
    await sleep(200)

    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    await sleep(3000)
    await mcpCall('refresh', {})

    // Root report.txt should have Documents content (overwritten)
    const rootContent = fs.readFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'report.txt'), 'utf-8')
    expect(rootContent).toContain('Quarterly report')

    // Source should be gone from Documents
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
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

    await mcpSwitchPane()
    await sleep(200)
    await mcpSwitchPane()
    await sleep(200)

    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await tauriPage.keyboard.press('F6')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 10000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage, 30000)

    await sleep(3000)

    // Root file unchanged (skip)
    const rootContent = fs.readFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'report.txt'), 'utf-8')
    expect(rootContent).toBe('root-version')

    // Source still exists (not moved because skipped)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(true)
  })
})
