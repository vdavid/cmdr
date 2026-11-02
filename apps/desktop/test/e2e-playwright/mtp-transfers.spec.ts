/**
 * E2E tests for moving and copying bytes between an MTP device and local disk:
 * cross-storage moves both ways, and 50 MB copies both ways.
 *
 * Requires the app to be built with `--features playwright-e2e,virtual-mtp`.
 * Shared setup and vocabulary: `mtp-helpers.ts`.
 */

import fs from 'fs'
import path from 'path'

import { test, expect } from './fixtures.js'
import { waitBudget } from './wait-budget.js'
import { MTP_FIXTURE_ROOT } from '../e2e-shared/mtp-fixtures.js'
import {
  mcpCall,
  getMtpVolumePath,
  mcpOpenMtpStorageRoot,
  mcpNavToPath,
  mcpAwaitItem,
  mcpConfirmedCopy,
  mcpConfirmedMove,
} from '../e2e-shared/mcp-client.js'
import { ensureAppReady, expectAndDismissToast, getFixtureRoot } from './helpers.js'
import { INTERNAL_STORAGE, installMtpSpecSetup, safeFileSize } from './mtp-helpers.js'

installMtpSpecSetup()

test.describe('MTP cross-storage move', () => {
  test('moves file from MTP to local', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Right pane is on local right/ (from ensureAppReady)
    // Move cursor to report.txt and move (F6)
    await mcpCall('select', { pane: 'left', names: ['report.txt'] })
    await mcpConfirmedMove('left')

    // Wait for the move to land on the local destination, then refresh the
    // pane so mcpAwaitItem sees the file.
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'report.txt')), { timeout: waitBudget(30000) })
      .toBeTruthy()
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'report.txt', 30)

    // Verify file arrived on local disk
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'report.txt'))).toBe(true)

    // Verify source removed from MTP backing dir
    // MTP move = copy + delete, so source should be gone
    await expect
      .poll(() => !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')), {
        timeout: waitBudget(15000),
      })
      .toBeTruthy()

    // Transfer fires a "Moved 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.', { timeout: waitBudget(30000) })
  })

  test('moves file from local to MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Left pane is on local left/ (has file-a.txt from fixtures)
    // Navigate right pane to MTP Internal Storage root
    await mcpOpenMtpStorageRoot('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Move cursor to file-a.txt in left pane and move
    await mcpCall('select', { pane: 'left', names: ['file-a.txt'] })
    await mcpConfirmedMove('left')

    // Wait for the move to land on the MTP backing dir, then refresh so the
    // pane re-lists.
    await expect
      .poll(() => fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt')), { timeout: waitBudget(30000) })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for file to appear in right pane (MTP root)
    await mcpAwaitItem('right', 'file-a.txt', 30)

    // Verify file arrived in MTP backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt'))).toBe(true)

    // Verify source removed from local disk
    await expect
      .poll(() => !fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt')), { timeout: waitBudget(15000) })
      .toBeTruthy()

    // Transfer fires a "Moved 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.', { timeout: waitBudget(30000) })
  })
})

test.describe('MTP large file transfer', () => {
  test('copies 50 MB file from local to MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Create a 50 MB file in local left/ (chunked write to avoid large buffer)
    const largePath = path.join(fixtureRoot, 'left', 'large-test.dat')
    const fd = fs.openSync(largePath, 'w')
    const chunk = Buffer.alloc(1024 * 1024, 0x42) // 1 MB of 'B'
    for (let i = 0; i < 50; i++) fs.writeSync(fd, chunk)
    fs.closeSync(fd)

    // Right pane: MTP Internal Storage root
    await mcpOpenMtpStorageRoot('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Re-navigate left pane so it picks up the new file (file watcher may be slow)
    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpAwaitItem('left', 'large-test.dat', 30)

    // Copy
    await mcpCall('select', { pane: 'left', names: ['large-test.dat'] })
    await mcpConfirmedCopy('left')

    // Poll until the destination file reaches the expected size (50 MB).
    const expectedSize = 50 * 1024 * 1024
    const destPath = path.join(MTP_FIXTURE_ROOT, 'internal', 'large-test.dat')
    await expect.poll(() => safeFileSize(destPath) === expectedSize, { timeout: waitBudget(30000) }).toBeTruthy()
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'large-test.dat', 60)

    // Verify file size in MTP backing dir
    const stat = fs.statSync(destPath)
    expect(stat.size).toBe(expectedSize)

    // Transfer fires a "Copied 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Copied 1 file.', { timeout: waitBudget(30000) })
  })

  test('copies 50 MB file from MTP to local', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Create a 50 MB file in MTP backing dir
    const largeMtpPath = path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'large-mtp.dat')
    const fd = fs.openSync(largeMtpPath, 'w')
    const chunk = Buffer.alloc(1024 * 1024, 0x43) // 1 MB of 'C'
    for (let i = 0; i < 50; i++) fs.writeSync(fd, chunk)
    fs.closeSync(fd)
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('rescan_virtual_mtp')`)

    // Left pane: MTP Documents
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'large-mtp.dat', 30)

    // Copy to local right/
    await mcpCall('select', { pane: 'left', names: ['large-mtp.dat'] })
    await mcpConfirmedCopy('left')

    // Poll until the destination file reaches the expected size (50 MB).
    const expectedSize = 50 * 1024 * 1024
    const destPath = path.join(fixtureRoot, 'right', 'large-mtp.dat')
    await expect.poll(() => safeFileSize(destPath) === expectedSize, { timeout: waitBudget(30000) }).toBeTruthy()
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'large-mtp.dat', 60)

    // Verify file size on local disk
    const stat = fs.statSync(destPath)
    expect(stat.size).toBe(expectedSize)

    // Transfer fires a "Copied 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Copied 1 file.', { timeout: waitBudget(30000) })
  })
})
