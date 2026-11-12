/**
 * E2E tests for MTP (Media Transfer Protocol) device integration: device
 * discovery, navigation, file operations (copy, move, delete, mkdir), and file
 * watching, through the full Cmdr stack: UI → Tauri IPC → MTP Volume trait →
 * virtual device.
 *
 * The rest of the MTP surface sits in sibling specs on the same shard:
 * `mtp-rename.spec.ts`, `mtp-transfers.spec.ts`, `mtp-refusals.spec.ts`, and the
 * scenario specs (`mtp-conflicts.spec.ts`, `mtp-delete-no-double-scan.spec.ts`, …).
 *
 * Requires the app to be built with `--features playwright-e2e,virtual-mtp`.
 * The virtual device's backing directory is `MTP_FIXTURE_ROOT` (run-scoped under
 * /tmp/cmdr-mtp-e2e-fixtures-<pid>/ when the checker launches the suite).
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
  mcpSelectVolume,
  mcpNavToPath,
  mcpAwaitItem,
  mcpNavToParent,
  mcpSwitchPane,
  mcpConfirmedCopy,
  mcpConfirmedMove,
  mcpConfirmedDelete,
} from '../e2e-shared/mcp-client.js'
import {
  dismissOverlay,
  ensureAppReady,
  expectAndDismissQuickLookHint,
  expectAndDismissToast,
  fileExistsInPane,
  focusPane,
  getFixtureRoot,
  isStateClean,
  moveCursorToFile,
  pressKey,
  MKDIR_DIALOG,
} from './helpers.js'
import { INTERNAL_STORAGE, LOCAL_VOLUME_NAME, SD_CARD, installMtpSpecSetup } from './mtp-helpers.js'

installMtpSpecSetup()

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe('MTP device discovery', () => {
  test('device appears in volume picker with both storages', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Open the volume picker by clicking the breadcrumb in the left pane
    await tauriPage.evaluate(`(function() {
            var pane = document.querySelectorAll('.file-pane')[0];
            var breadcrumb = pane ? pane.querySelector('.volume-breadcrumb .volume-name') : null;
            if (!breadcrumb) breadcrumb = document.querySelector('.volume-breadcrumb .volume-name');
            if (breadcrumb) breadcrumb.click();
        })()`)

    // Wait for the dropdown to appear
    await expect.poll(async () => tauriPage.isVisible('[data-menu]'), { timeout: waitBudget(5000) }).toBeTruthy()

    // Wait for "Mobile" category label to appear (MTP volumes load reactively)
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('[data-menu] [data-menu-heading]');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === 'Mobile') return true;
            }
            return false;
        })()`),
        { timeout: waitBudget(10000) },
      )
      .toBeTruthy()

    // Check that Internal Storage is listed
    const hasInternal = await tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('[data-menu] .volume-label');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === ${JSON.stringify(INTERNAL_STORAGE)}) return true;
            }
            return false;
        })()`)
    expect(hasInternal).toBe(true)

    // Check that SD Card is listed
    const hasSdCard = await tauriPage.evaluate<boolean>(`(function() {
            var labels = document.querySelectorAll('[data-menu] .volume-label');
            for (var i = 0; i < labels.length; i++) {
                if (labels[i].textContent.trim() === ${JSON.stringify(SD_CARD)}) return true;
            }
            return false;
        })()`)
    expect(hasSdCard).toBe(true)

    // Close the dropdown (and assert it closed; the global afterEach would
    // otherwise fail the test for the leak).
    await dismissOverlay(tauriPage)
  })
})

test.describe('MTP navigation', () => {
  test('browses MTP files and navigates back', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Select Internal Storage on left pane
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
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

  test('switching back to a storage reopens the folder last used there', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    const mtpPath = await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Leave the phone, then pick the storage again with no path of our own: the pane
    // asks the phone whether `Documents` still exists and goes back there.
    await mcpSelectVolume('left', LOCAL_VOLUME_NAME)
    await expect
      .poll(async () => isStateClean(tauriPage, LOCAL_VOLUME_NAME), { timeout: waitBudget(5000) })
      .toBeTruthy()
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'report.txt')
    expect(await fileExistsInPane(tauriPage, 'notes.txt', 0)).toBe(true)
  })

  test('free space is displayed for MTP volume', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Select Internal Storage on left pane
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Open the volume picker to check space info
    await tauriPage.evaluate(`(function() {
            var breadcrumb = document.querySelector('.volume-breadcrumb .volume-name');
            if (breadcrumb) breadcrumb.click();
        })()`)
    await expect.poll(async () => tauriPage.isVisible('[data-menu]'), { timeout: waitBudget(5000) }).toBeTruthy()

    // Poll for space info: MTP space data may load asynchronously after dropdown opens
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`(function() {
            var items = document.querySelectorAll('[data-menu] [data-menu-row]');
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
        { timeout: waitBudget(15000) },
      )
      .toBeTruthy()

    // Close the volume picker dropdown (asserts closure via dismissOverlay's poll).
    await dismissOverlay(tauriPage)
  })
})

test.describe('MTP file operations', () => {
  test('copies file from MTP to local', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Navigate left pane to MTP Documents
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Right pane is on local right/ (from ensureAppReady)
    // Move cursor to report.txt and copy
    await mcpCall('select', { pane: 'left', names: ['report.txt'] })
    await mcpConfirmedCopy('left')

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
    await expect.poll(() => fs.existsSync(destPath), { timeout: waitBudget(30000) }).toBeTruthy()

    // Force the pane to re-list so the await reads a fresh PaneStateStore.
    await mcpCall('refresh', {})
    await mcpAwaitItem('right', 'report.txt', 30)

    // Verify source still exists (copy, not move)
    await mcpSwitchPane()
    await mcpAwaitItem('left', 'report.txt')

    // Transfer fires a "Copied 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Copied 1 file.', { timeout: waitBudget(30000) })
  })

  test('copies file from local to MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Left pane is on local left/ (has file-a.txt from fixtures)
    // Navigate right pane to MTP Internal Storage root
    await mcpOpenMtpStorageRoot('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Cursor file-a.txt in left pane and copy
    await mcpCall('select', { pane: 'left', names: ['file-a.txt'] })
    await mcpConfirmedCopy('left')

    // MTP transfer is fire-and-forget. Poll the backing dir until the file
    // lands, then force a refresh so the pane re-lists.
    await expect
      .poll(() => fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt')), { timeout: waitBudget(30000) })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for file to appear in right pane (MTP root)
    await mcpAwaitItem('right', 'file-a.txt', 30)

    // Verify in MTP backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'file-a.txt'))).toBe(true)

    // Transfer fires a "Copied 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Copied 1 file.', { timeout: waitBudget(30000) })
  })

  test('moves file between MTP directories', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'notes.txt')

    // Navigate right pane to MTP Music
    await mcpOpenMtpStorageRoot('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')
    await mcpNavToPath('right', `${mtpPath}/Music`)

    await focusPane(tauriPage, 0)

    // Confirm left pane is still showing Documents content after the focus.
    await mcpAwaitItem('left', 'notes.txt')

    // Move cursor to notes.txt and move it
    await mcpCall('select', { pane: 'left', names: ['notes.txt'] })
    await mcpConfirmedMove('left')

    // MTP move is fire-and-forget. Poll for the backing-dir state (source gone,
    // dest present) before triggering the refresh that drives the pane re-listing.
    await expect
      .poll(
        () =>
          !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt')) &&
          fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Music', 'notes.txt')),
        { timeout: waitBudget(30000) },
      )
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for notes.txt to disappear from Documents (left pane)
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'notes.txt', 0)), { timeout: waitBudget(15000) })
      .toBeTruthy()

    // Wait for notes.txt to appear in Music (right pane)
    await mcpAwaitItem('right', 'notes.txt', 30)

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(false)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Music', 'notes.txt'))).toBe(true)

    // Transfer fires a "Moved 1 file." toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Moved 1 file.', { timeout: waitBudget(30000) })
  })

  test('deletes file on MTP with the permanent-delete dialog', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // Move cursor to report.txt via keyboard (to test full keyboard flow)
    await moveCursorToFile(tauriPage, 'report.txt')

    // Press F8 to open delete dialog (we want to inspect the dialog)
    await pressKey(tauriPage, 'F8')
    await tauriPage.waitForSelector('[data-dialog-id="delete-confirmation"]', 10000)

    // Verify the dialog shows "Delete" (not "Move to trash") for MTP
    const confirmLabel = await tauriPage.evaluate<string>(`(function() {
            var dialog = document.querySelector('[data-dialog-id="delete-confirmation"]');
            if (!dialog) return '';
            var btn = dialog.querySelector('.btn-primary, .btn-danger');
            return btn ? btn.textContent.trim() : '';
        })()`)
    expect(confirmLabel).toBe('Delete')

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
      .poll(async () => !(await tauriPage.isVisible('[data-dialog-id="delete-confirmation"]')), {
        timeout: waitBudget(10000),
      })
      .toBeTruthy()

    // MTP delete is fire-and-forget. Poll the backing dir until the file is gone.
    await expect
      .poll(() => !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')), {
        timeout: waitBudget(30000),
      })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for report.txt to disappear from the UI listing
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), { timeout: waitBudget(15000) })
      .toBeTruthy()

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)

    // Transfer fires a "Delete complete" toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Delete complete', { timeout: waitBudget(30000) })
  })

  test('deletes multiple selected files on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents (has report.txt and notes.txt)
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
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
        { timeout: waitBudget(2000) },
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
        { timeout: waitBudget(2000) },
      )
      .toBeTruthy()

    // The first Space press fires the persistent Quick Look hint toast (macOS only) (Cmdr's
    // Finder-convert reminder). Both Space presses share one toast because
    // the hint dedupes by id while it's already on screen. Dismiss the hint
    // before the delete so the safety net doesn't conflate it with the
    // Delete-complete toast we expect below.
    await expectAndDismissQuickLookHint(tauriPage)

    // Delete via MCP, standing in for the person who confirms
    await mcpConfirmedDelete('left')

    // MTP multi-delete is fire-and-forget. Poll the backing dir.
    await expect
      .poll(
        () =>
          !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt')) &&
          !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt')),
        { timeout: waitBudget(30000) },
      )
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for both files to disappear
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'report.txt', 0)), { timeout: waitBudget(15000) })
      .toBeTruthy()
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'notes.txt', 0)), { timeout: waitBudget(15000) })
      .toBeTruthy()

    // Verify on backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'report.txt'))).toBe(false)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'Documents', 'notes.txt'))).toBe(false)

    // Transfer fires a "Delete complete" toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Delete complete', { timeout: waitBudget(30000) })
  })

  test('deletes folder with nested files recursively on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage root (has DCIM folder with nested files)
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'DCIM')

    // Verify DCIM has nested content before delete
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', 'photo-001.jpg'))).toBe(true)
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', 'Burst', 'burst-001.jpg'))).toBe(true)

    // Move cursor to DCIM and delete
    await mcpCall('move_cursor', { pane: 'left', filename: 'DCIM' })
    await mcpConfirmedDelete('left')

    // MTP recursive delete is fire-and-forget. Poll the backing dir.
    await expect
      .poll(() => !fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM')), { timeout: waitBudget(45000) })
      .toBeTruthy()
    await mcpCall('refresh', {})

    // Wait for DCIM to disappear from listing
    await expect
      .poll(async () => !(await fileExistsInPane(tauriPage, 'DCIM', 0)), { timeout: waitBudget(15000) })
      .toBeTruthy()

    // Verify entire tree gone from backing dir
    expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM'))).toBe(false)

    // Transfer fires a "Delete complete" toast on success; assert + dismiss
    // pins the user-facing confirmation and clears the leak guard.
    await expectAndDismissToast(tauriPage, 'Delete complete', { timeout: waitBudget(30000) })
  })

  test('creates folder on MTP', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage root
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Create folder via MCP: mkdir opens the dialog, then we type the name and confirm
    await mcpCall('mkdir', { pane: 'left' })
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} input.text-field-control`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} input.text-field-control`, 'NewFolder')
    // Wait for the OK button to enable in response to the typed name.
    await expect
      .poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: waitBudget(2000) })
      .toBeTruthy()
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)

    // Wait for dialog to close
    await expect
      .poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: waitBudget(5000) })
      .toBeTruthy()

    // MTP mkdir is fire-and-forget. Poll the backing dir for the folder.
    await expect
      .poll(() => fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'NewFolder')), { timeout: waitBudget(15000) })
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

test.describe('MTP file watching', () => {
  test('detects externally added file in MTP backing dir', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate left pane to MTP Documents
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    await mcpNavToPath('left', `${mtpPath}/Documents`)
    await mcpAwaitItem('left', 'report.txt')

    // This is the one test that exercises the live-watch pipeline, so resume
    // the watcher (the beforeEach leaves it paused). By now the beforeEach's
    // recreate FSEvents have long drained during the pause, so resuming here
    // only surfaces the single clean write below — no stale-event race.
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('resume_virtual_mtp_watcher')`)

    // Write a new file directly to the backing dir (simulating external change).
    // The virtual device watches the backing dir and emits ObjectAdded events,
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
