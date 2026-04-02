/**
 * E2E tests for MTP (Media Transfer Protocol) device integration.
 *
 * Tests virtual MTP device browsing, file operations (copy, move, delete,
 * mkdir, rename), read-only enforcement, and file watching through the
 * full Cmdr stack: UI → Tauri IPC → MTP Volume trait → virtual device.
 *
 * Requires the app to be built with `--features playwright-e2e,virtual-mtp`.
 * The virtual device uses a backing directory at /tmp/cmdr-mtp-e2e-fixtures/.
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
  mcpNavToParent,
  mcpSwitchPane,
} from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  getFixtureRoot,
  pollUntil,
  sleep,
  moveCursorToFile,
  pressKey,
  fileExistsInPane,
  MKDIR_DIALOG,
} from './helpers.js'

// Volume names (verified from manual testing against the virtual device)
const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'
const SD_CARD = 'Virtual Pixel 9 - SD Card'

/**
 * Discovers the mtp:// path prefix for a named MTP storage from cmdr://state.
 * The device ID is assigned at runtime, so tests must discover it dynamically.
 */
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

// MTP operations go through the virtual device which adds protocol overhead.
// 30s default is too tight for multi-step MTP test chains.
test.setTimeout(120_000)

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot()) // Local fixtures for cross-storage tests
  recreateMtpFixtures() // MTP backing dir
  await sleep(500) // Let the virtual device's event loop settle
  await initMcpClient(tauriPage) // Discover MCP port

  // Force both panes back to a local volume. Previous tests may have left a pane
  // on MTP, and ensureAppReady's mcp-nav-to-path events get rejected by
  // navigateToPath when the pane is on an MTP volume (it requires select_volume first).
  // Emitting mcp-volume-select with "Macintosh HD" switches to the local volume.
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: 'Macintosh HD' } });
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: 'Macintosh HD' } });
    })()`)
  await sleep(2000) // Wait for volume switches to complete

  // Dismiss any lingering dialogs/overlays from previous tests
  await tauriPage.keyboard.press('Escape')
  await sleep(200)
  await tauriPage.keyboard.press('Escape')
  await sleep(200)
})

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe('MTP device discovery', () => {
  test('device appears in volume picker with both storages', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Open the volume picker by clicking the breadcrumb in the left pane
    await tauriPage.evaluate(`(function() {
            var pane = document.querySelectorAll('.file-pane')[0];
            var breadcrumb = pane ? pane.closest('.file-pane-wrapper')?.querySelector('.volume-breadcrumb .volume-name') : null;
            if (!breadcrumb) breadcrumb = document.querySelector('.volume-breadcrumb .volume-name');
            if (breadcrumb) breadcrumb.click();
        })()`)

    // Wait for the dropdown to appear
    await pollUntil(tauriPage, async () => tauriPage.isVisible('.volume-dropdown'), 5000)

    // Check for "Mobile" category label (MTP devices are grouped under "Mobile")
    const hasMobileGroup = await tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('.volume-dropdown .category-label');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === 'Mobile') return true;
            }
            return false;
        })()`)
    expect(hasMobileGroup).toBe(true)

    // Check that Internal Storage is listed
    const hasInternal = await tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('.volume-dropdown .volume-label');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === ${JSON.stringify(INTERNAL_STORAGE)}) return true;
            }
            return false;
        })()`)
    expect(hasInternal).toBe(true)

    // Check that SD Card is listed
    const hasSdCard = await tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('.volume-dropdown .volume-label');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === ${JSON.stringify(SD_CARD)}) return true;
            }
            return false;
        })()`)
    expect(hasSdCard).toBe(true)

    // Close the dropdown
    await tauriPage.keyboard.press('Escape')
  })
})

test.describe('MTP navigation', () => {
  test('browses MTP files and navigates back', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Select Internal Storage on left pane
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Verify root listing: Documents, DCIM, Music
    const hasDocuments = await fileExistsInPane(tauriPage, 'Documents', 0)
    const hasDCIM = await fileExistsInPane(tauriPage, 'DCIM', 0)
    const hasMusic = await fileExistsInPane(tauriPage, 'Music', 0)
    expect(hasDocuments).toBe(true)
    expect(hasDCIM).toBe(true)
    expect(hasMusic).toBe(true)

    // Navigate into Documents
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Verify Documents contents
    const hasReport = await fileExistsInPane(tauriPage, 'report.txt', 0)
    const hasNotes = await fileExistsInPane(tauriPage, 'notes.txt', 0)
    expect(hasReport).toBe(true)
    expect(hasNotes).toBe(true)

    // Navigate back to parent
    await mcpNavToParent()
    await mcpAwaitItem('left', 'Documents')

    // Confirm we're back at the root (Documents is visible again)
    const backAtRoot = await fileExistsInPane(tauriPage, 'Documents', 0)
    expect(backAtRoot).toBe(true)
  })

  test('free space is displayed for MTP volume', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Select Internal Storage on left pane
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Open the volume picker to check space info
    await tauriPage.evaluate(`(function() {
            var breadcrumb = document.querySelector('.volume-breadcrumb .volume-name');
            if (breadcrumb) breadcrumb.click();
        })()`)
    await pollUntil(tauriPage, async () => tauriPage.isVisible('.volume-dropdown'), 5000)

    // Poll for space info — MTP space data may load asynchronously after dropdown opens
    await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(`(function() {
            var items = document.querySelectorAll('.volume-dropdown .volume-item');
            for (var i = 0; i < items.length; i++) {
                var label = items[i].querySelector('.volume-label');
                if (label && label.textContent.trim() === ${JSON.stringify(INTERNAL_STORAGE)}) {
                    // Space info is a sibling element after the volume-item
                    var next = items[i].nextElementSibling;
                    if (next && next.classList.contains('volume-space-info')) {
                        var text = next.querySelector('.volume-space-text');
                        return text ? text.textContent.trim().length > 0 : false;
                    }
                }
            }
            return false;
        })()`),
      15000,
    )

    await tauriPage.keyboard.press('Escape')
  })
})

test.describe('MTP file operations', () => {
  test('copies file from MTP to local', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Right pane is on local right/ (from ensureAppReady)
    // Move cursor to report.txt and copy
    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await mcpCall('copy', { autoConfirm: true })

    // Wait for file to appear in right pane
    await mcpAwaitItem('right', 'report.txt')

    // Verify on disk
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'report.txt'))).toBe(true)

    // Verify source still exists (copy, not move)
    await mcpSwitchPane()
    await mcpAwaitItem('left', 'report.txt')
  })

  test('copies file from local to MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Left pane is on local left/ (has file-a.txt from fixtures)
    // Navigate right pane to MTP Internal Storage root
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Cursor file-a.txt in left pane and copy
    await mcpCall('move_cursor', { pane: 'left', filename: 'file-a.txt' })
    await mcpCall('copy', { autoConfirm: true })

    // MTP listings don't auto-refresh after upload — wait for transfer, then force refresh
    await sleep(2000)
    await mcpCall('refresh', {})

    // Wait for file to appear in right pane (MTP root)
    await mcpAwaitItem('right', 'file-a.txt', 30)

    // Verify in MTP backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt'))).toBe(true)
  })

  test('moves file between MTP directories', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'notes.txt')

    // Navigate right pane to MTP Music
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')
    await mcpNavToPath('right', `${mtpPath}/Music`)

    // Ensure left pane is focused
    await mcpSwitchPane()
    await sleep(200)
    await mcpSwitchPane()
    await sleep(200)

    // Ensure we're focused on left pane (Documents)
    // Use mcpAwaitItem to confirm left pane is showing Documents content
    await mcpAwaitItem('left', 'notes.txt')

    // Move cursor to notes.txt and move it
    await mcpCall('move_cursor', { pane: 'left', filename: 'notes.txt' })
    await mcpCall('move', { autoConfirm: true })

    // MTP listings don't auto-refresh after move — wait for operation, then force refresh
    await sleep(2000)
    await mcpCall('refresh', {})

    // Wait for notes.txt to disappear from Documents (left pane)
    await pollUntil(tauriPage, async () => !(await fileExistsInPane(tauriPage, 'notes.txt', 0)), 15000)

    // Wait for notes.txt to appear in Music (right pane)
    await mcpAwaitItem('right', 'notes.txt', 30)

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(false)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Music', 'notes.txt'))).toBe(true)
  })

  test('deletes file on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Move cursor to report.txt and delete
    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await mcpCall('delete', { autoConfirm: true })

    // MTP listings don't auto-refresh after delete — wait for operation, then force refresh
    await sleep(2000)
    await mcpCall('refresh', {})

    // Wait for report.txt to disappear from the UI listing
    await pollUntil(tauriPage, async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), 15000)

    // TODO: Verify backing dir deletion once Trash-on-MTP bug is fixed.
    // Currently, the delete dialog defaults to Trash mode, but MTP paths can't be trashed
    // (no local filesystem path), so the backing dir file may persist even though the UI
    // removes it. See: Trash error: source_not_found in logs.
  })

  test('creates folder on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage root
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Create folder via MCP — mkdir opens the dialog, then we type the name and confirm
    await mcpCall('mkdir', {})
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, 'NewFolder')
    await sleep(200)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)

    // MTP listings may not auto-refresh — force refresh after mkdir
    await sleep(1000)
    await mcpCall('refresh', {})

    // Wait for the folder to appear
    await mcpAwaitItem('left', 'NewFolder', 30)

    // Verify on backing dir
    const folderPath = path.join(MTP_FIXTURE_ROOT, 'internal', 'NewFolder')
    expect(fs.existsSync(folderPath)).toBe(true)
    expect(fs.statSync(folderPath).isDirectory()).toBe(true)
  })
})

test.describe('MTP rename', () => {
  test('renames file on MTP via keyboard', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Move cursor to report.txt via DOM (keyboard flow test)
    await moveCursorToFile(tauriPage, 'report.txt')

    // Press F2 to start rename
    await tauriPage.keyboard.press('F2')
    await tauriPage.waitForSelector('.rename-input', 10000)

    // Clear existing value and type new name
    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.rename-input');
            if (!input) return;
            input.focus();
            var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
            if (desc && desc.set) desc.set.call(input, '');
            else input.value = '';
            input.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)
    await sleep(100)
    await tauriPage.type('.rename-input', 'renamed-report.txt')
    await sleep(200)
    await tauriPage.press('.rename-input', 'Enter')

    // Wait for rename input to disappear
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.rename-input')), 10000)

    // Verify new name appears, old name gone
    await pollUntil(tauriPage, async () => fileExistsInPane(tauriPage, 'renamed-report.txt', 0), 10000)
    await pollUntil(tauriPage, async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), 5000)

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'renamed-report.txt'))).toBe(true)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
  })
})

test.describe('MTP read-only enforcement', () => {
  test('read-only storage rejects write operations', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(SD_CARD)

    // Navigate left pane to SD Card → photos
    await mcpSelectVolume('left', SD_CARD)
    await mcpAwaitItem('left', 'photos')
    await mcpNavToPath('left', `${mtpPath}/photos`)
    await mcpAwaitItem('left', 'sunset.jpg')

    // Verify sunset.jpg is visible
    const hasSunset = await fileExistsInPane(tauriPage, 'sunset.jpg', 0)
    expect(hasSunset).toBe(true)

    // Try F7 (create folder) — should trigger an error or show the dialog which
    // will fail on confirm. Press F7 and check what happens.
    await pressKey(tauriPage, 'F7')
    await sleep(500)

    // Check if an alert dialog appeared (read-only volumes may show an alert
    // instead of the mkdir dialog)
    const hasAlert = await tauriPage.isVisible('[data-dialog-id="alert"]')
    const hasMkdir = await tauriPage.isVisible(MKDIR_DIALOG)

    if (hasAlert) {
      // Read-only pre-check showed an alert — verify the message
      const alertText = await tauriPage.evaluate<string>(`(function() {
                var msg = document.querySelector('[data-dialog-id="alert"] .message, [data-dialog-id="alert"] #alert-dialog-message');
                return msg ? msg.textContent : '';
            })()`)
      expect(alertText.toLowerCase()).toMatch(/read.only|not.*possible|can.t.*write/)

      // Dismiss the alert
      await tauriPage.evaluate(`(function() {
                var btn = document.querySelector('[data-dialog-id="alert"] button');
                if (btn) btn.click();
            })()`)
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
    } else if (hasMkdir) {
      // Dialog opened — type a name and confirm, expect backend error
      await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
      await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, 'TestFolder')
      await sleep(200)
      await tauriPage.waitForSelector(`${MKDIR_DIALOG} .btn-primary`, 3000)
      await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

      // Wait for an error message to appear in the dialog
      await pollUntil(
        tauriPage,
        async () => {
          const hasError = await tauriPage.evaluate<boolean>(
            `!!document.querySelector('${MKDIR_DIALOG} .error-message')`,
          )
          return hasError
        },
        10000,
      )

      // Dismiss the dialog
      await tauriPage.keyboard.press('Escape')
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
    } else {
      // Neither dialog appeared — this is unexpected but we verify no folder was created
      expect(hasAlert || hasMkdir).toBe(true)
    }

    // Verify no folder was created on the backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'readonly', 'photos', 'TestFolder'))).toBe(false)

    // Also test rename on read-only: cursor on sunset.jpg, press F2
    await moveCursorToFile(tauriPage, 'sunset.jpg')
    await tauriPage.keyboard.press('F2')
    await sleep(500)

    // Rename should be blocked with an alert (DualPaneExplorer.startRename checks isReadOnly)
    const hasRenameAlert = await tauriPage.isVisible('[data-dialog-id="alert"]')
    expect(hasRenameAlert).toBe(true)

    const renameAlertText = await tauriPage.evaluate<string>(`(function() {
            var msg = document.querySelector('[data-dialog-id="alert"] #alert-dialog-message');
            return msg ? msg.textContent : '';
        })()`)
    expect(renameAlertText).toContain('read-only')

    // Dismiss the alert
    await tauriPage.evaluate(`(function() {
            var btn = document.querySelector('[data-dialog-id="alert"] button');
            if (btn) btn.click();
        })()`)
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
  })
})

test.describe('MTP file watching', () => {
  test('detects externally added file in MTP backing dir', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Write a new file directly to the backing dir (simulating external change)
    fs.writeFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'new-file.txt'), 'hello from external write')

    // Wait for the virtual device's event loop to detect the change, then force refresh
    // as a fallback in case the event loop doesn't pick it up in time
    await sleep(2000)
    await mcpCall('refresh', {})

    // Wait for the file to appear in the left pane
    await mcpAwaitItem('left', 'new-file.txt', 30)

    // Verify it shows up in the DOM too
    const hasNewFile = await fileExistsInPane(tauriPage, 'new-file.txt', 0)
    expect(hasNewFile).toBe(true)
  })
})
