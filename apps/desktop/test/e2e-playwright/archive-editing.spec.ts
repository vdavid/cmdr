/**
 * E2E tests for EDITING a zip in place (zip-as-folder writes).
 *
 * Zip is the one writable archive format, so every flow here runs the managed
 * archive-edit path: creating a folder, renaming, deleting (permanent, with no
 * Trash), pasting in, moving out, and answering a name clash. The data-safety
 * halves are the point — a cancelled paste leaves the original zip intact, and
 * an edit never drops an untouched sibling — because the mutator rewrites the
 * whole archive through temp+rename.
 *
 * Reading an archive (Enter policy, listing, preview, extract-out, and the
 * read-only formats) is `archive-browsing.spec.ts`; the two share
 * `archive-helpers.ts`.
 *
 * Fixture (at $CMDR_E2E_START_PATH, recreated per test):
 *   left/
 *     sample.zip            <- a real zip: inner.txt + nested/deep.txt
 *     file-a.txt, file-b.txt, sub-dir/, bulk/, ...
 *   right/                  <- empty
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpReadResource } from '../e2e-shared/mcp-client.js'
import {
  clickButtonByText,
  ensureAppReady,
  flushFileWatcher,
  getFixtureRoot,
  settleFocusedPaneOnLeft,
  moveCursorToFile,
  fileExistsInFocusedPane,
  expectAndDismissToast,
  TRANSFER_DIALOG,
  MKDIR_DIALOG,
} from './helpers.js'
import {
  enterEntry,
  navigatePaneTo,
  setArchiveEnterBehavior,
  settleOnFixtureLeft,
  type PageLike,
} from './archive-helpers.js'

const DELETE_DIALOG = '[data-dialog-id="delete-confirmation"]'
const TRANSFER_PROGRESS = '[data-dialog-id="transfer-progress"]'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// Putting the shared `left/` + `right/` tree back is this spec's job: the
// post-test leak guard fails whoever leaves it dirty, and the restore is
// surgical, so it only rewrites what actually drifted.
test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

/** Dismisses every open toast. For flows whose completion-toast wording is
 *  timing-dependent (a cancel that may or may not have caught the write; a
 *  conflict resolution), so the global afterEach doesn't fail on a leaked toast. */
async function clearAllToasts(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
      var closes = document.querySelectorAll('.toast .toast-close');
      for (var i = 0; i < closes.length; i++) closes[i].click();
  })()`)
  await expect
    .poll(async () => tauriPage.evaluate<boolean>(`document.querySelectorAll('.toast').length === 0`), {
      timeout: 3000,
    })
    .toBeTruthy()
}

test.describe('Archive editing', () => {
  // These tests edit INSIDE the zip, so force the zip Enter behavior to Browse
  // (the default is Ask, which would pop the menu instead — that flow is covered
  // by `archive-browsing.spec.ts`).
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await settleOnFixtureLeft(tauriPage, 'sample.zip')
    await setArchiveEnterBehavior({ zip: 'browse', bundle: 'browse' })
  })

  test('creating a folder inside the archive adds it and shows it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    // F7 inside a zip runs the real managed archive-edit flow (no refusal).
    const folderName = `zip-folder-${String(Date.now())}`
    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} input.text-field-control`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} input.text-field-control`, folderName)
    await expect.poll(async () => tauriPage.isEnabled(`${MKDIR_DIALOG} .btn-primary`), { timeout: 2000 }).toBeTruthy()
    await tauriPage.click(`${MKDIR_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    // The archive rewrite lands async; the live-watch refresh then shows the new
    // folder inside the zip. Probe for it, don't sleep.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, folderName), { timeout: 10000 }).toBeTruthy()
  })

  test('renaming a file inside the archive works', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F2')
    await tauriPage.waitForSelector('.rename-input', 3000)
    // Clear the input (native setter + input event) then type the new name.
    await tauriPage.evaluate(`(function() {
            var input = document.querySelector('.rename-input');
            if (!input) return;
            input.focus();
            var desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
            if (desc && desc.set) desc.set.call(input, ''); else input.value = '';
            input.dispatchEvent(new Event('input', { bubbles: true }));
        })()`)
    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`document.querySelector('.rename-input')?.value === ''`), {
        timeout: 2000,
      })
      .toBeTruthy()
    await tauriPage.type('.rename-input', 'inner-renamed.txt')
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`document.querySelector('.rename-input')?.value === 'inner-renamed.txt'`),
        { timeout: 3000 },
      )
      .toBeTruthy()
    await tauriPage.press('.rename-input', 'Enter')
    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 5000 }).toBeTruthy()

    // The rewrite lands async; the refresh shows the new name and drops the old.
    await expect
      .poll(async () => fileExistsInFocusedPane(tauriPage, 'inner-renamed.txt'), { timeout: 10000 })
      .toBeTruthy()
    await expect
      .poll(async () => !(await fileExistsInFocusedPane(tauriPage, 'inner.txt')), { timeout: 10000 })
      .toBeTruthy()
  })

  test('deleting a file inside the archive is permanent (no Trash) and removes it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    // F8 preselects Trash, but an archive forces permanent: the dialog shows the
    // archive warning and no "Move to trash" switch.
    await tauriPage.keyboard.press('F8')
    await tauriPage.waitForSelector(DELETE_DIALOG, 5000)
    const bannerText = await tauriPage.textContent(`${DELETE_DIALOG} .warning-banner`)
    expect(bannerText).toContain('no trash inside an archive')
    expect(await tauriPage.isVisible(`${DELETE_DIALOG} .switch-root`)).toBe(false)

    // Confirm the permanent delete (danger button) and wait for the rewrite.
    await expect.poll(async () => tauriPage.isEnabled(`${DELETE_DIALOG} .btn-danger`), { timeout: 5000 }).toBeTruthy()
    await tauriPage.click(`${DELETE_DIALOG} .btn-danger`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 10000 }).toBeTruthy()
    await expectAndDismissToast(tauriPage, 'Delete complete')
    await expect
      .poll(async () => !(await fileExistsInFocusedPane(tauriPage, 'inner.txt')), { timeout: 10000 })
      .toBeTruthy()
    // A sibling entry survives the edit (an edit never drops an untouched sibling).
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'nested'), { timeout: 5000 }).toBeTruthy()
  })

  test('pasting a file into the archive lands it inside the zip', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Right pane inside the zip (the copy destination).
    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

    // Navigating the right pane focuses it (focus follows the navigated pane), so
    // re-focus the left source pane before the F5 copy reads from it.
    await navigatePaneTo(tauriPage, 'left', `${fixtureRoot}/left`)
    await settleFocusedPaneOnLeft(tauriPage, `${fixtureRoot}/left`)

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 15000 }).toBeTruthy()
    await expectAndDismissToast(tauriPage, 'file')

    // Re-read the zip from disk in the LEFT pane (still at `left/`, cursor on
    // `file-a.txt`): entering it lists the inner entries, which now include the
    // pasted file — proof it landed inside the archive.
    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'file-a.txt'), { timeout: 10000 }).toBeTruthy()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
  })

  test('MOVING a file into the archive root lands it inside and removes the original', async ({ tauriPage }) => {
    // F6, not F5, and that single keypress is the whole point of this test.
    // Copy always routes cross-volume, so it can't tell a correct archive-root
    // destination from a broken one. Move has the local `moveFiles` fast path, and
    // the destination pane sitting AT `sample.zip` is the one shape that regresses
    // when the destination is asked the NARROW archive question: the fast path
    // runs and the backend refuses with "Destination must be a directory".
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Right pane AT the archive root — where Enter on a zip lands you.
    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

    await navigatePaneTo(tauriPage, 'left', `${fixtureRoot}/left`)
    await settleFocusedPaneOnLeft(tauriPage, `${fixtureRoot}/left`)

    const found = await moveCursorToFile(tauriPage, 'file-b.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F6')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 15000 }).toBeTruthy()
    await expectAndDismissToast(tauriPage, 'file')

    // A move, so the original is gone from disk...
    await expect
      .poll(() => !fs.existsSync(path.join(fixtureRoot, 'left', 'file-b.txt')), { timeout: 10000 })
      .toBeTruthy()
    // ...and the entry is inside the archive.
    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'file-b.txt'), { timeout: 10000 }).toBeTruthy()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
  })

  test('cancelling a paste into the archive leaves the zip contents intact', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const leftDir = path.join(fixtureRoot, 'left')
    const zipPath = path.join(leftDir, 'sample.zip')

    // `set_test_throttle` paces the archive rewrite between entries
    // (`archive_edit/engine.rs`), and that pause is the window Cancel lands in.
    // Unpaced, a local paste into this small zip lands before the progress dialog
    // can offer an enabled Cancel, which quietly turns this into a completed-copy
    // test. The window comes from the pacing, not the source's size, so it stays small.
    const pasteName = 'paste-to-cancel.txt'
    fs.writeFileSync(path.join(leftDir, pasteName), 'a paste that never lands\n')
    // The pane has to SEE this write before the F5 below can cursor it, and an
    // external create reaches it only via FSEvents, which can lag or drop it under
    // load. `flushFileWatcher` re-reads the listing through the Volume trait instead
    // of waiting on delivery, so the wait can't be lost.
    await flushFileWatcher(tauriPage)

    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

    // Navigating the right pane focuses it (focus follows the navigated pane), so
    // re-focus the left source pane (still at `left/` from the beforeEach) before
    // cursoring the file for the F5 copy.
    await navigatePaneTo(tauriPage, 'left', `${fixtureRoot}/left`)
    await settleFocusedPaneOnLeft(tauriPage, `${fixtureRoot}/left`)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, pasteName), { timeout: 5000 }).toBeTruthy()
    const found = await moveCursorToFile(tauriPage, pasteName)
    expect(found).toBe(true)

    // "Intact" below means the same bytes, and no temp left beside the zip.
    const zipBytesBefore = fs.readFileSync(zipPath)
    const leftNamesBefore = fs.readdirSync(leftDir).sort()

    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: 500 })`)
    try {
      await tauriPage.keyboard.press('F5')
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
      await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
      await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
      await tauriPage.waitForSelector(TRANSFER_PROGRESS, 5000)
      await clickButtonByText(tauriPage, `${TRANSFER_PROGRESS} button`, 'Cancel')
      await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 20000 }).toBeTruthy()
      await clearAllToasts(tauriPage)
    } finally {
      await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
    }

    // Temp+rename leaves the original alone until the final rename, and a cancel
    // before it removes the temp: the zip is byte-identical, with nothing beside it.
    expect(fs.readFileSync(zipPath).equals(zipBytesBefore)).toBe(true)
    expect(fs.readdirSync(leftDir).sort()).toEqual(leftNamesBefore)

    // The zip's prior contents are fully intact regardless of when the cancel
    // caught the edit (temp+rename never mutates the original until the final
    // atomic rename): re-enter and assert both original entries survive.
    await navigatePaneTo(tauriPage, 'left', `${fixtureRoot}/left/sample.zip`)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 10000 }).toBeTruthy()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'nested'), { timeout: 5000 }).toBeTruthy()
  })

  test('moving a file OUT of the archive removes it from the zip and lands it locally', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    // F6 moves the entry OUT to the right pane (a compound extract + archive delete).
    await tauriPage.keyboard.press('F6')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 15000 }).toBeTruthy()
    await expectAndDismissToast(tauriPage, 'file')

    // Landed on disk in the right pane's folder...
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'inner.txt')), { timeout: 10000 })
      .toBeTruthy()
    // ...and removed from the zip (the focused pane is still inside it; the live
    // watch refreshes the listing).
    await expect
      .poll(async () => !(await fileExistsInFocusedPane(tauriPage, 'inner.txt')), { timeout: 10000 })
      .toBeTruthy()
  })

  test('pasting a name that already exists inside the zip prompts a conflict', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // A source file whose name collides with an existing zip entry (`inner.txt`).
    fs.writeFileSync(path.join(fixtureRoot, 'left', 'inner.txt'), 'local copy that clashes')
    // Re-read the listing rather than waiting on FSEvents to deliver the external
    // create; see the sibling cancel test above.
    await flushFileWatcher(tauriPage)

    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

    // Navigating the right pane focuses it (focus follows the navigated pane), so
    // re-focus the left source pane before cursoring the clashing file.
    await navigatePaneTo(tauriPage, 'left', `${fixtureRoot}/left`)
    await settleFocusedPaneOnLeft(tauriPage, `${fixtureRoot}/left`)

    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    // Default policy is "Ask for each", so starting surfaces the inline conflict UI.
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await tauriPage.waitForSelector(TRANSFER_PROGRESS, 5000)
    await expect.poll(async () => tauriPage.isVisible('.conflict-section'), { timeout: 8000 }).toBeTruthy()
    const conflictName = await tauriPage.textContent('.conflict-section .conflict-filename')
    expect(conflictName).toContain('inner.txt')

    // The prompt appearing is the point of this test. Resolve it (overwrite) so
    // the op settles and the dialog closes, then clear the completion toast.
    await tauriPage.evaluate(`(function(){
        var btns = Array.prototype.slice.call(document.querySelectorAll('.conflict-buttons-row button'));
        var pick = btns.find(function(b){ return /^overwrite$/i.test((b.textContent||'').trim()); }) || btns[0];
        if (pick) pick.click();
    })()`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 15000 }).toBeTruthy()
    await clearAllToasts(tauriPage)
  })
})
