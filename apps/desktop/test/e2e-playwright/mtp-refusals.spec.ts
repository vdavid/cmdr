/**
 * E2E tests for what an MTP device refuses: the clipboard (⌘C / ⌘X / ⌘V), which
 * can't carry files off a device, and a read-only storage, which turns every
 * write away.
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
} from '../e2e-shared/mcp-client.js'
import {
  clickEntryInPane,
  dismissOverlay,
  ensureAppReady,
  expectAndDismissToast,
  fileExistsInPane,
  moveCursorToFile,
  pressKey,
  CTRL_OR_META,
  MKDIR_DIALOG,
} from './helpers.js'
import { INTERNAL_STORAGE, SD_CARD, installMtpSpecSetup } from './mtp-helpers.js'

installMtpSpecSetup()

test.describe('MTP clipboard rejection', () => {
  test('Cmd+C on MTP file shows rejection toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Focus the left pane and move cursor to Documents
    await mcpCall('move_cursor', { pane: 'left', filename: 'Documents' })
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')?.getAttribute('data-filename') === 'Documents'`,
          ),
        { timeout: waitBudget(2000) },
      )
      .toBeTruthy()

    // Press Cmd+C (copy to clipboard). Toast appears asynchronously; the
    // helper polls for the message and dismisses it after asserting.
    await pressKey(tauriPage, `${CTRL_OR_META}+c`)
    await expectAndDismissToast(tauriPage, 'The clipboard can’t carry files from this device. Use F5 to copy them.', {
      timeout: waitBudget(5000),
    })
  })

  test('Cmd+X on MTP file shows rejection toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate left pane to MTP Internal Storage
    await mcpOpenMtpStorageRoot('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')

    // Focus and move cursor
    await mcpCall('move_cursor', { pane: 'left', filename: 'Documents' })
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')?.getAttribute('data-filename') === 'Documents'`,
          ),
        { timeout: waitBudget(2000) },
      )
      .toBeTruthy()

    // Press Cmd+X (cut to clipboard). Toast appears asynchronously; the
    // helper polls for the message and dismisses it after asserting.
    await pressKey(tauriPage, `${CTRL_OR_META}+x`)
    await expectAndDismissToast(tauriPage, 'The clipboard can’t carry files from this device. Use F6 to move them.', {
      timeout: waitBudget(5000),
    })
  })

  test('Cmd+V into MTP folder shows rejection toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch right pane to MTP
    await mcpOpenMtpStorageRoot('right', INTERNAL_STORAGE)
    await mcpAwaitItem('right', 'Documents')

    // Switch focus to right pane (paste targets the focused pane).
    // Click on the right pane to ensure DOM focus matches app state.
    await clickEntryInPane(tauriPage, 1)
    // Wait for the right pane to be the focused pane.
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelectorAll('.file-pane')[1]?.classList.contains('is-focused') === true`,
          ),
        { timeout: waitBudget(3000) },
      )
      .toBeTruthy()

    // Verify right pane is focused (has MTP volume)
    const rightFocused = await tauriPage.evaluate<boolean>(`(function(){
            var pane = document.querySelectorAll('.file-pane')[1];
            return pane ? pane.classList.contains('is-focused') : false;
        })()`)
    expect(rightFocused).toBe(true)

    // Dispatch Cmd+V (macOS) / Ctrl+V (Linux) via trusted keyboard events,
    // then assert+dismiss the rejection toast.
    await tauriPage.keyboard.down(CTRL_OR_META)
    await tauriPage.keyboard.press('v')
    await tauriPage.keyboard.up(CTRL_OR_META)
    await expectAndDismissToast(tauriPage, 'Use F5 to copy files onto this device.', { timeout: waitBudget(5000) })
  })
})

test.describe('MTP read-only enforcement', () => {
  test('read-only storage rejects write operations', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(SD_CARD)

    // Navigate left pane to SD Card → photos
    await mcpOpenMtpStorageRoot('left', SD_CARD)
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
        { timeout: waitBudget(5000) },
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
      await expect
        .poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: waitBudget(5000) })
        .toBeTruthy()
    } else if (hasMkdir) {
      // Dialog opened. Type a name and confirm, expect backend error.
      await tauriPage.waitForSelector(`${MKDIR_DIALOG} input.text-field-control`, 3000)
      await tauriPage.fill(`${MKDIR_DIALOG} input.text-field-control`, 'TestFolder')
      await expect
        .poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: waitBudget(2000) })
        .toBeTruthy()
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
          { timeout: waitBudget(10000) },
        )
        .toBeTruthy()

      // Dismiss the dialog
      await dismissOverlay(tauriPage)
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
    await expect
      .poll(async () => tauriPage.isVisible('[data-dialog-id="alert"]'), { timeout: waitBudget(5000) })
      .toBeTruthy()

    // Rename should be blocked with an alert (DualPaneExplorer.startRename checks mountIsReadOnly)
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
    await expect
      .poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: waitBudget(5000) })
      .toBeTruthy()
  })
})
