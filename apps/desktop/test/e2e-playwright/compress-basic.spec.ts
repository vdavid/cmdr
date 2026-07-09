/**
 * E2E tests for the Compress command (⌥F5): pack the cursor item into a NEW zip
 * in the other pane, via the Transfer dialog's third mode.
 *
 * Covers the two flows that matter end to end:
 *  - Happy path: trigger Compress, the dialog opens in Compress mode with a
 *    `.zip` suggestion, confirm, the zip lands in the other pane, and browsing
 *    INTO it (archive-as-folder) shows the source inside.
 *  - Cancel safety: cancel mid-compress; the target is at worst a valid empty
 *    archive (temp+rename never tears the file), never partial garbage, and no
 *    `.cmdr-tmp-` scratch survives.
 *
 * A REMOTE destination (compress onto an SMB/MTP share) is NOT covered here: the
 * dialog and confirm path don't branch on local-vs-remote (they just pass a
 * destination path), so a remote E2E would only re-exercise this same UI while
 * needing an SMB share mounted into the Playwright harness. The remote
 * seed-through-volume round-trip is covered by the Rust integration test
 * `smb_integration_compress_local_files_onto_the_share` (a real Docker Samba
 * share) plus the `compress_remote_tests` unit suite (both swap shapes).
 *
 * Fixture (at $CMDR_E2E_START_PATH, recreated per test): `left/` holds
 * `file-a.txt`, `file-b.txt`, `sub-dir/`, etc.; `right/` is empty.
 */

import { randomBytes } from 'crypto'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  fileExistsInFocusedPane,
  getFixtureRoot,
  moveCursorToFile,
  TRANSFER_DIALOG,
} from './helpers.js'

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const TRANSFER_PROGRESS = '[data-dialog-id="transfer-progress"]'

/** Navigate a pane to a path via the same `mcp-nav-to-path` event the MCP server
 *  uses. Focus follows the navigated pane, so the focused-pane helpers then read
 *  it (mirrors `archive-browsing.spec.ts`). */
async function navigatePaneTo(tauriPage: PageLike, pane: 'left' | 'right', targetPath: string): Promise<void> {
  await tauriPage.evaluate(`(function () {
        window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: ${JSON.stringify(pane)}, path: ${JSON.stringify(targetPath)} }
        });
    })()`)
}

/**
 * Forces zip Enter behavior to Browse through the same MCP `set_setting` path the
 * UI uses, so navigating into the produced archive steps inside instead of popping
 * the Ask menu. `set_setting` round-trips, so it's live by the time this resolves.
 */
async function setArchiveBrowse(): Promise<void> {
  await mcpCall('set_setting', {
    id: 'behavior.archiveEnterBehavior',
    value: JSON.stringify({ zip: 'browse', bundle: 'browse' }),
  })
}

/** Reads the Compress dialog's path-input value (the editable `.zip` target). */
async function readPathInput(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(document.querySelector('${TRANSFER_DIALOG} .path-input')?.value || '')`)
}

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.describe('Compress (⌥F5)', () => {
  test('compressing a file opens the dialog in Compress mode and packs it into the other pane', async ({
    tauriPage,
  }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await setArchiveBrowse()
    const fixtureRoot = getFixtureRoot()
    const destZip = path.join(fixtureRoot, 'right', 'file-a.txt.zip')

    // Cursor a real file in the left pane, then trigger the compress command
    // (the ⌥F5 handler → openCompressDialog, same path the menu/palette hit).
    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)
    await dispatchMenuCommand(tauriPage, 'file.compress')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    // The dialog is in Compress mode: the third toggle is active...
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<string>(
            `(document.querySelector('${TRANSFER_DIALOG} .operation-toggle .toggle-option.active')?.textContent || '').trim()`,
          ),
        { timeout: 3000 },
      )
      .toBe('Compress')
    // ...and the editable path field defaults to a `.zip` in the OTHER pane's folder.
    const suggested = await readPathInput(tauriPage)
    expect(suggested.endsWith('/file-a.txt.zip')).toBe(true)
    expect(suggested.startsWith(path.join(fixtureRoot, 'right'))).toBe(true)

    // Confirm.
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 15000 }).toBeTruthy()
    // The completion toast is part of the contract; asserting it also clears it so
    // the afterEach leak guard doesn't fail on a lingering transient toast.
    await expectAndDismissToast(tauriPage, 'Compressed')

    // The zip landed on disk in the other pane's folder and is a valid archive.
    await expect.poll(() => fs.existsSync(destZip), { timeout: 5000 }).toBeTruthy()
    expect(fs.readFileSync(destZip).subarray(0, 2).toString('latin1')).toBe('PK')

    // Browsing INTO the produced zip (archive-as-folder) shows the source inside.
    await navigatePaneTo(tauriPage, 'left', destZip)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'file-a.txt'), { timeout: 10000 }).toBeTruthy()
  })

  test('cancelling a compress leaves at worst a valid empty archive, never a torn file', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const destZip = path.join(fixtureRoot, 'right', 'big-to-cancel.dat.zip')

    // A large, INCOMPRESSIBLE source gives a real window to cancel mid-write.
    // Random bytes don't deflate away (all-same-byte data compresses in a blink
    // and the op finishes before the cancel lands), so the ~24 MB payload is
    // load-bearing, not a stray sleep. Don't shrink it or make it compressible to
    // "speed it up": that turns this into a completed-compress test and stops
    // exercising the cancel path. Created directly (the shared bulk cache isn't
    // populated for a single manual instance).
    const bigName = 'big-to-cancel.dat'
    fs.writeFileSync(path.join(fixtureRoot, 'left', bigName), randomBytes(24 * 1024 * 1024))

    const found = await moveCursorToFile(tauriPage, bigName)
    expect(found).toBe(true)
    await dispatchMenuCommand(tauriPage, 'file.compress')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)

    // Cancel as soon as the progress dialog appears. temp+rename means the target
    // is only ever the valid empty seed until the final atomic rename, so a cancel
    // can never leave a torn file.
    await tauriPage.waitForSelector(TRANSFER_PROGRESS, 5000)
    await tauriPage.evaluate(`(function(){
        var dlg = document.querySelector('${TRANSFER_PROGRESS}');
        var btns = dlg ? Array.prototype.slice.call(dlg.querySelectorAll('button')) : [];
        var cancel = btns.find(function(b){ return /cancel/i.test((b.textContent||'')); });
        if (cancel) cancel.click();
    })()`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 20000 }).toBeTruthy()
    // The cancel may or may not have caught the write, so the completion-toast
    // wording is timing-dependent: dismiss whatever's there so the leak guard passes.
    await tauriPage.evaluate(`(function(){
        var closes = document.querySelectorAll('.toast .toast-close');
        for (var i = 0; i < closes.length; i++) closes[i].click();
    })()`)
    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`document.querySelectorAll('.toast').length === 0`), {
        timeout: 3000,
      })
      .toBeTruthy()

    // Data-safety assertion: the target is either absent or a valid archive (its
    // bytes start with the zip signature `PK`), regardless of when the cancel
    // caught the edit. It is NEVER a partial, unopenable file.
    if (fs.existsSync(destZip)) {
      expect(fs.readFileSync(destZip).subarray(0, 2).toString('latin1')).toBe('PK')
    }
    // No temp scratch survives the cancel.
    const rightDir = path.join(fixtureRoot, 'right')
    const leftover = fs.readdirSync(rightDir).filter((n) => n.includes('.cmdr-tmp-'))
    expect(leftover, `.cmdr-tmp- scratch left under ${rightDir}: ${leftover.join(', ')}`).toEqual([])
  })
})
