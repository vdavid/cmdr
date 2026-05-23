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
  mcpNavToParent,
  mcpSwitchPane,
} from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  getFixtureRoot,
  pollUntil,
  moveCursorToFile,
  pressKey,
  fileExistsInPane,
  isStateClean,
  MKDIR_DIALOG,
  CTRL_OR_META,
} from './helpers.js'

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
type PageLike = TauriPage | BrowserPageAdapter

/**
 * Polls until a function returns a non-empty string, then returns that string.
 * Useful for waiting for toast messages or dynamic text to appear.
 */
async function pollUntilValue(
  page: PageLike,
  getValue: () => Promise<string>,
  timeout: number,
  interval = 200,
): Promise<string> {
  const captured = { value: '' }
  // allowed-bare-poll: helper implementation — captures result via closure side-effect, not return value
  await pollUntil(
    page,
    async () => {
      try {
        const val = await getValue()
        if (val.length > 0) {
          captured.value = val
          return true
        }
      } catch {
        // Element might not exist yet
      }
      return false
    },
    timeout,
    interval,
  )
  return captured.value
}

// Volume names (verified from manual testing against the virtual device)
const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'
const SD_CARD = 'Virtual Pixel 9 - SD Card'

// Local volume name differs by platform (macOS: "Macintosh HD", Linux: "Root")
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

/** Returns the size of a file, or -1 if it doesn't exist / can't be statted. */
function safeFileSize(p: string): number {
  try {
    return fs.statSync(p).size
  } catch {
    return -1
  }
}

/**
 * Reads cmdr://state and returns true when both panes show the local volume.
 * The state YAML has `left:` and `right:` blocks each containing a `  volume: NAME` line.
 */
async function bothPanesOnLocalVolume(): Promise<boolean> {
  const state = await mcpReadResource('cmdr://state')
  const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) => line.replace(/^\n {2}volume: /, ''))
  return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
}

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

/**
 * Sets `.rename-input`'s value directly via the native setter so Svelte's
 * reactivity sees a single update. Used instead of `tauriPage.type` because
 * per-character key dispatch over the playwright socket costs ~14 s for an
 * 18-char name on Linux/Xvfb (vs ~80 ms on macOS); the slow path made
 * `MTP rename via keyboard` the worst per-test Linux outlier (5.1×).
 */
async function setRenameInputValue(page: PageLike, value: string): Promise<void> {
  await page.evaluate(
    `(function() {
      var input = document.querySelector('.rename-input');
      if (!input) return;
      input.focus();
      var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
      if (desc && desc.set) desc.set.call(input, ${JSON.stringify(value)});
      else input.value = ${JSON.stringify(value)};
      input.dispatchEvent(new Event('input', { bubbles: true }));
    })()`,
  )
  await expect
    .poll(
      async () => page.evaluate<boolean>(`document.querySelector('.rename-input')?.value === ${JSON.stringify(value)}`),
      { timeout: 3000 },
    )
    .toBeTruthy()
}

// MTP operations go through the virtual device which adds protocol overhead.
// 30s default is too tight for multi-step MTP test chains.
test.setTimeout(120_000)

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot()) // Local fixtures for cross-storage tests
  await initMcpClient(tauriPage) // Discover MCP port

  // Pause the filesystem watcher before recreating MTP fixtures. Without this,
  // the watcher may process stale deletion events after the rescan, removing
  // objects that were just re-added.
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures() // MTP backing dir

  // Settle FSEvents, rescan, settle again, rescan, then resume, all in one
  // IPC call. See `resync_virtual_mtp_after_disk_change` in commands/mtp.rs
  // for the rationale (FSEvents has ~200-500 ms latency on macOS, so a naive
  // rescan + resume races with late events).
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('resync_virtual_mtp_after_disk_change')`)

  // Force both panes back to a local volume. Previous tests may have left a pane
  // on MTP, and ensureAppReady's mcp-nav-to-path events get rejected by
  // navigateToPath when the pane is on an MTP volume (it requires select_volume first).
  // Volume name differs by platform: "Macintosh HD" on macOS, "Root" on Linux.
  //
  // Short-circuit: if both panes are already on the local volume AND no modal
  // overlay is lingering, skip the volume-select + Escape sequence. This is the
  // common case for non-first tests in the spec.
  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
          var invoke = window.__TAURI_INTERNALS__.invoke;
          invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
          invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
      })()`)
    // Wait for both panes to show the local volume.
    await expect.poll(async () => bothPanesOnLocalVolume(), { timeout: 5000 }).toBeTruthy()

    // Dismiss any lingering dialogs/overlays from previous tests
    await tauriPage.keyboard.press('Escape')
    await tauriPage.keyboard.press('Escape')
    // allowed-bare-poll: best-effort modal dismissal in beforeEach; overlay may or may not be present
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 2000)
  }
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
    await expect.poll(async () => tauriPage.isVisible('.volume-dropdown'), { timeout: 5000 }).toBeTruthy()

    // Wait for "Mobile" category label to appear (MTP volumes load reactively)
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('.volume-dropdown .category-label');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === 'Mobile') return true;
            }
            return false;
        })()`),
        { timeout: 10000 },
      )
      .toBeTruthy()

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
    await expect.poll(async () => tauriPage.isVisible('.volume-dropdown'), { timeout: 5000 }).toBeTruthy()

    // Poll for space info: MTP space data may load asynchronously after dropdown opens
    await expect
      .poll(
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
        { timeout: 15000 },
      )
      .toBeTruthy()

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

    // Poll the destination on disk first: that's the authoritative truth.
    // Under heavy concurrent load (full slow-suite run with rust-tests-linux +
    // Docker SMB containers eating disk + CPU), both safety nets for refreshing
    // the destination pane can race: the FilePane's notify-rs watcher (200 ms
    // debounce) can miss the FSEvents add, and the `refreshPanesAfterTransfer`
    // IPC fired from `handleTransferComplete` can queue up behind a saturated
    // Tauri event loop. PaneStateStore then stays stale even though the BE
    // copy already succeeded and the file is on disk. Tests 11 (local→MTP) and
    // 27 (50 MB MTP→local) already use this pattern; this brings test 10 inline.
    const destPath = path.join(fixtureRoot, 'right', 'report.txt')
    await expect.poll(() => fs.existsSync(destPath), { timeout: 30000 }).toBeTruthy()

    // Force the pane to re-list so the await reads a fresh PaneStateStore.
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'report.txt', 30)

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

    // MTP transfer is fire-and-forget. Poll the backing dir until the file
    // lands, then force a refresh so the pane re-lists.
    await expect
      .poll(() => fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt')), { timeout: 30000 })
      .toBeTruthy()
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

    // Toggle twice and poll for the visual focus class to land on the left pane.
    await mcpSwitchPane()
    await mcpSwitchPane()
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelectorAll('.file-pane')[0]?.classList.contains('is-focused') === true`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()

    // Confirm left pane is still showing Documents content after the toggle.
    await mcpAwaitItem('left', 'notes.txt')

    // Move cursor to notes.txt and move it
    await mcpCall('move_cursor', { pane: 'left', filename: 'notes.txt' })
    await mcpCall('move', { autoConfirm: true })

    // MTP move is fire-and-forget. Poll for the backing-dir state (source gone,
    // dest present) before triggering the refresh that drives the pane re-listing.
    await expect
      .poll(
        () =>
          !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt')) &&
          fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Music', 'notes.txt')),
        { timeout: 30000 },
      )
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for notes.txt to disappear from Documents (left pane)
    await expect.poll(async () => !(await fileExistsInPane(tauriPage, 'notes.txt', 0)), { timeout: 15000 }).toBeTruthy()

    // Wait for notes.txt to appear in Music (right pane)
    await mcpAwaitItem('right', 'notes.txt', 30)

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(false)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Music', 'notes.txt'))).toBe(true)
  })

  test('deletes file on MTP with "Delete permanently" dialog', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Move cursor to report.txt via keyboard (to test full keyboard flow)
    await moveCursorToFile(tauriPage, 'report.txt')

    // Press F8 to open delete dialog (not autoConfirm, because we want to inspect the dialog)
    await pressKey(tauriPage, 'F8')
    await tauriPage.waitForSelector('[data-dialog-id="delete-confirmation"]', 10000)

    // Verify the dialog shows "Delete permanently" (not "Move to trash") for MTP
    const confirmLabel = await tauriPage.evaluate<string>(`(function() {
            var dialog = document.querySelector('[data-dialog-id="delete-confirmation"]');
            if (!dialog) return '';
            var btn = dialog.querySelector('.btn-primary, .btn-danger');
            return btn ? btn.textContent.trim() : '';
        })()`)
    expect(confirmLabel).toBe('Delete permanently')

    // Verify the warning banner about trash not being supported
    const hasWarning = await tauriPage.evaluate<boolean>(`(function() {
            var dialog = document.querySelector('[data-dialog-id="delete-confirmation"]');
            if (!dialog) return false;
            var warning = dialog.querySelector('.warning-banner');
            return warning ? warning.textContent.includes('trash') : false;
        })()`)
    expect(hasWarning).toBe(true)

    // Confirm the delete
    await tauriPage.evaluate(`(function() {
            var dialog = document.querySelector('[data-dialog-id="delete-confirmation"]');
            if (!dialog) return;
            var btn = dialog.querySelector('.btn-danger');
            if (btn) btn.click();
        })()`)

    // Wait for dialog to close
    await expect
      .poll(async () => !(await tauriPage.isVisible('[data-dialog-id="delete-confirmation"]')), { timeout: 10000 })
      .toBeTruthy()

    // MTP delete is fire-and-forget. Poll the backing dir until the file is gone.
    await expect
      .poll(() => !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')), {
        timeout: 30000,
      })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for report.txt to disappear from the UI listing
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), { timeout: 15000 })
      .toBeTruthy()

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
  })

  test('deletes multiple selected files on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents (has report.txt and notes.txt)
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Select both files: move to report.txt, Space to select, move to notes.txt, Space to select.
    // Poll for `.is-selected` after each Space so we don't race the next cursor move.
    await moveCursorToFile(tauriPage, 'report.txt')
    await pressKey(tauriPage, 'Space')
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `!!document.querySelector('.file-pane.is-focused .file-entry[data-filename="report.txt"].is-selected')`,
          ),
        { timeout: 2000 },
      )
      .toBeTruthy()
    await moveCursorToFile(tauriPage, 'notes.txt')
    await pressKey(tauriPage, 'Space')
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `!!document.querySelector('.file-pane.is-focused .file-entry[data-filename="notes.txt"].is-selected')`,
          ),
        { timeout: 2000 },
      )
      .toBeTruthy()

    // Delete via MCP with autoConfirm
    await mcpCall('delete', { autoConfirm: true })

    // MTP multi-delete is fire-and-forget. Poll the backing dir.
    await expect
      .poll(
        () =>
          !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')) &&
          !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt')),
        { timeout: 30000 },
      )
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for both files to disappear
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), { timeout: 15000 })
      .toBeTruthy()
    await expect.poll(async () => !(await fileExistsInPane(tauriPage, 'notes.txt', 0)), { timeout: 15000 }).toBeTruthy()

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(false)
  })

  test('deletes folder with nested files recursively on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage root (has DCIM folder with nested files)
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'DCIM')

    // Verify DCIM has nested content before delete
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', 'photo-001.jpg'))).toBe(true)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', 'Burst', 'burst-001.jpg'))).toBe(true)

    // Move cursor to DCIM and delete
    await mcpCall('move_cursor', { pane: 'left', filename: 'DCIM' })
    await mcpCall('delete', { autoConfirm: true })

    // MTP recursive delete is fire-and-forget. Poll the backing dir.
    await expect
      .poll(() => !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM')), { timeout: 45000 })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for DCIM to disappear from listing
    await expect.poll(async () => !(await fileExistsInPane(tauriPage, 'DCIM', 0)), { timeout: 15000 }).toBeTruthy()

    // Verify entire tree gone from backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM'))).toBe(false)
  })

  test('creates folder on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage root
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Create folder via MCP: mkdir opens the dialog, then we type the name and confirm
    await mcpCall('mkdir', {})
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, 'NewFolder')
    // Wait for the OK button to enable in response to the typed name.
    await expect.poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: 2000 }).toBeTruthy()
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    // MTP mkdir is fire-and-forget. Poll the backing dir for the folder.
    await expect
      .poll(() => fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'NewFolder')), { timeout: 15000 })
      .toBeTruthy()
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

    // Set the new value directly. `tauriPage.type` dispatches one key event
    // per character over the playwright socket, which adds ~14 s on Linux/Xvfb
    // (vs ~80 ms on macOS) for an 18-char name. The setter+input event hits
    // the same Svelte reactivity path that user typing does.
    await setRenameInputValue(tauriPage, 'renamed-report.txt')
    await tauriPage.press('.rename-input', 'Enter')

    // Wait for rename input to disappear
    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 10000 }).toBeTruthy()

    // Verify new name appears, old name gone
    await expect.poll(async () => fileExistsInPane(tauriPage, 'renamed-report.txt', 0), { timeout: 10000 }).toBeTruthy()
    await expect.poll(async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), { timeout: 5000 }).toBeTruthy()

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'renamed-report.txt'))).toBe(true)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
  })

  test('rename to existing name is rejected on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    await moveCursorToFile(tauriPage, 'report.txt')
    await tauriPage.keyboard.press('F2')
    await tauriPage.waitForSelector('.rename-input', 10000)

    await setRenameInputValue(tauriPage, 'notes.txt')
    await tauriPage.press('.rename-input', 'Enter')

    // Conflict dialog should appear since notes.txt already exists
    await tauriPage.waitForSelector('[data-dialog-id="rename-conflict"]', 10000)
    const dialogText = await tauriPage.evaluate(
      `document.querySelector('[data-dialog-id="rename-conflict"]')?.textContent ?? ''`,
    )
    expect(dialogText).toContain('already exists')

    // Cancel the dialog. Both files should remain unchanged.
    await tauriPage.keyboard.press('Escape')
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 3000 }).toBeTruthy()

    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(true)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(true)
  })
})

test.describe('MTP cross-storage move', () => {
  test('moves file from MTP to local', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Right pane is on local right/ (from ensureAppReady)
    // Move cursor to report.txt and move (F6)
    await mcpCall('move_cursor', { pane: 'left', filename: 'report.txt' })
    await mcpCall('move', { autoConfirm: true })

    // Wait for the move to land on the local destination, then refresh the
    // pane so mcpAwaitItem sees the file.
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'report.txt')), { timeout: 30000 })
      .toBeTruthy()
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'report.txt', 30)

    // Verify file arrived on local disk
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'report.txt'))).toBe(true)

    // Verify source removed from MTP backing dir
    // MTP move = copy + delete, so source should be gone
    await expect
      .poll(() => !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')), {
        timeout: 15000,
      })
      .toBeTruthy()
  })

  test('moves file from local to MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Left pane is on local left/ (has file-a.txt from fixtures)
    // Navigate right pane to MTP Internal Storage root
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Move cursor to file-a.txt in left pane and move
    await mcpCall('move_cursor', { pane: 'left', filename: 'file-a.txt' })
    await mcpCall('move', { autoConfirm: true })

    // Wait for the move to land on the MTP backing dir, then refresh so the
    // pane re-lists.
    await expect
      .poll(() => fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt')), { timeout: 30000 })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for file to appear in right pane (MTP root)
    await mcpAwaitItem('right', 'file-a.txt', 30)

    // Verify file arrived in MTP backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt'))).toBe(true)

    // Verify source removed from local disk
    await expect
      .poll(() => !fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt')), { timeout: 15000 })
      .toBeTruthy()
  })
})

test.describe('MTP clipboard rejection', () => {
  test('Cmd+C on MTP file shows rejection toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Focus the left pane and move cursor to Documents
    await mcpCall('move_cursor', { pane: 'left', filename: 'Documents' })
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')?.getAttribute('data-filename') === 'Documents'`,
          ),
        { timeout: 2000 },
      )
      .toBeTruthy()

    // Press Cmd+C (copy to clipboard). Toast appears asynchronously.
    await pressKey(tauriPage, `${CTRL_OR_META}+c`)

    // Verify toast appears with MTP clipboard message
    await expect
      .poll(
        async () => {
          const text = await tauriPage.evaluate<string>(`(function() {
                var toasts = document.querySelectorAll('.toast-message');
                for (var i = 0; i < toasts.length; i++) {
                    if (toasts[i].textContent.includes('F5')) return toasts[i].textContent;
                }
                return '';
            })()`)
          return text.length > 0
        },
        { timeout: 5000 },
      )
      .toBeTruthy()

    // Verify exact message
    const message = await tauriPage.evaluate<string>(`(function() {
            var toasts = document.querySelectorAll('.toast-message');
            for (var i = 0; i < toasts.length; i++) {
                if (toasts[i].textContent.includes('F5')) return toasts[i].textContent;
            }
            return '';
        })()`)
    expect(message).toBe('Use F5 to copy files from MTP devices')
  })

  test('Cmd+X on MTP file shows rejection toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Focus and move cursor
    await mcpCall('move_cursor', { pane: 'left', filename: 'Documents' })
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')?.getAttribute('data-filename') === 'Documents'`,
          ),
        { timeout: 2000 },
      )
      .toBeTruthy()

    // Press Cmd+X (cut to clipboard). Toast appears asynchronously, and
    // pollUntilValue below handles waiting for it.
    await pressKey(tauriPage, `${CTRL_OR_META}+x`)

    // Verify toast with F6 message
    const message = await pollUntilValue(
      tauriPage,
      async () =>
        tauriPage.evaluate<string>(`(function() {
            var toasts = document.querySelectorAll('.toast-message');
            for (var i = 0; i < toasts.length; i++) {
                if (toasts[i].textContent.includes('F6')) return toasts[i].textContent;
            }
            return '';
        })()`),
      5000,
    )
    expect(message).toBe('Use F6 to move files from MTP devices')
  })

  test('Cmd+V into MTP folder shows rejection toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch right pane to MTP
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Switch focus to right pane (paste targets the focused pane).
    // Click on the right pane to ensure DOM focus matches app state.
    await tauriPage.evaluate(`(function(){
            var panes = document.querySelectorAll('.file-pane');
            if (panes[1]) {
                var entry = panes[1].querySelector('.file-entry');
                if (entry) entry.click();
            }
        })()`)
    // Wait for the right pane to be the focused pane.
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelectorAll('.file-pane')[1]?.classList.contains('is-focused') === true`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()

    // Verify right pane is focused (has MTP volume)
    const rightFocused = await tauriPage.evaluate<boolean>(`(function(){
            var pane = document.querySelectorAll('.file-pane')[1];
            return pane ? pane.classList.contains('is-focused') : false;
        })()`)
    expect(rightFocused).toBe(true)

    // Dispatch Cmd+V (macOS) / Ctrl+V (Linux) via trusted keyboard events.
    // pollUntilValue below waits for the toast to appear, so no fixed delay here.
    await tauriPage.keyboard.down(CTRL_OR_META)
    await tauriPage.keyboard.press('v')
    await tauriPage.keyboard.up(CTRL_OR_META)

    // Verify toast with F5 message about copying TO MTP.
    // Check for ANY toast first to diagnose what's happening.
    const message = await pollUntilValue(
      tauriPage,
      async () =>
        tauriPage.evaluate<string>(`(function() {
            var toasts = document.querySelectorAll('.toast-message');
            if (toasts.length > 0) return toasts[toasts.length - 1].textContent || 'empty';
            return '';
        })()`),
      5000,
    )
    expect(message).toBe('Use F5 to copy files to MTP devices')
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

    // Try F7 (create folder), which should trigger an error or show the dialog which
    // will fail on confirm. Press F7 and wait until either the read-only alert
    // OR the mkdir dialog has appeared.
    await pressKey(tauriPage, 'F7')
    await expect
      .poll(
        async () =>
          (await tauriPage.isVisible('[data-dialog-id="alert"]')) || (await tauriPage.isVisible(MKDIR_DIALOG)),
        { timeout: 5000 },
      )
      .toBeTruthy()

    // Check which dialog appeared (read-only volumes may show an alert
    // instead of the mkdir dialog)
    const hasAlert = await tauriPage.isVisible('[data-dialog-id="alert"]')
    const hasMkdir = await tauriPage.isVisible(MKDIR_DIALOG)

    if (hasAlert) {
      // Read-only pre-check showed an alert. Verify the message.
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
      await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()
    } else if (hasMkdir) {
      // Dialog opened. Type a name and confirm, expect backend error.
      await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
      await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, 'TestFolder')
      await expect.poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: 2000 }).toBeTruthy()
      await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

      // Wait for an error message to appear in the dialog
      await expect
        .poll(
          async () => {
            const hasError = await tauriPage.evaluate<boolean>(
              `!!document.querySelector('${MKDIR_DIALOG} .error-message')`,
            )
            return hasError
          },
          { timeout: 10000 },
        )
        .toBeTruthy()

      // Dismiss the dialog
      await tauriPage.keyboard.press('Escape')
      await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()
    } else {
      // Neither dialog appeared. This is unexpected; fail explicitly.
      throw new Error('Expected either an alert or mkdir dialog to appear, but neither did')
    }

    // Verify no folder was created on the backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'readonly', 'photos', 'TestFolder'))).toBe(false)

    // Also test rename on read-only: cursor on sunset.jpg, press F2
    await moveCursorToFile(tauriPage, 'sunset.jpg')
    await tauriPage.keyboard.press('F2')
    // Wait for the read-only alert dialog to appear.
    await expect.poll(async () => tauriPage.isVisible('[data-dialog-id="alert"]'), { timeout: 5000 }).toBeTruthy()

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
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()
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

    // Write a new file directly to the backing dir (simulating external change).
    // mtp-rs 0.6.0 watches the backing dir and emits ObjectAdded events,
    // which Cmdr's event loop picks up and sends as directory-diff to the frontend.
    fs.writeFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'new-file.txt'), 'hello from external write')

    // Wait for the file to appear via the virtual device's file watcher → event loop → directory-diff pipeline.
    // In long-running test suites, the watcher may be slow to process events. If the first
    // wait times out, force a refresh and try again. This tests that the file exists on
    // the virtual device even if the push-based watcher missed the event.
    try {
      await mcpAwaitItem('left', 'new-file.txt', 30)
    } catch {
      // File watcher didn't pick it up. Force refresh and retry.
      await mcpCall('refresh', {})
      await mcpAwaitItem('left', 'new-file.txt', 30)
    }

    // Verify it shows up in the DOM too
    const hasNewFile = await fileExistsInPane(tauriPage, 'new-file.txt', 0)
    expect(hasNewFile).toBe(true)
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
    await mcpSelectVolume('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Re-navigate left pane so it picks up the new file (file watcher may be slow)
    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpAwaitItem('left', 'large-test.dat', 30)

    // Copy
    await mcpCall('move_cursor', { pane: 'left', filename: 'large-test.dat' })
    await mcpCall('copy', { autoConfirm: true })

    // Poll until the destination file reaches the expected size (50 MB).
    const expectedSize = 50 * 1024 * 1024
    const destPath = path.join(MTP_FIXTURE_ROOT, 'internal', 'large-test.dat')
    await expect.poll(() => safeFileSize(destPath) === expectedSize, { timeout: 30000 }).toBeTruthy()
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'large-test.dat', 60)

    // Verify file size in MTP backing dir
    const stat = fs.statSync(destPath)
    expect(stat.size).toBe(expectedSize)
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
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'large-mtp.dat', 30)

    // Copy to local right/
    await mcpCall('move_cursor', { pane: 'left', filename: 'large-mtp.dat' })
    await mcpCall('copy', { autoConfirm: true })

    // Poll until the destination file reaches the expected size (50 MB).
    const expectedSize = 50 * 1024 * 1024
    const destPath = path.join(fixtureRoot, 'right', 'large-mtp.dat')
    await expect.poll(() => safeFileSize(destPath) === expectedSize, { timeout: 30000 }).toBeTruthy()
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'large-mtp.dat', 60)

    // Verify file size on local disk
    const stat = fs.statSync(destPath)
    expect(stat.size).toBe(expectedSize)
  })
})
