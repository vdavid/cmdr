/**
 * E2E for duplicating in place: a copy that lands in the folder its source
 * already lives in.
 *
 * Paste ends a single-item duplicate in the inline rename editor. The Duplicate
 * command (⌘D, the menu, the palette) asks nothing. F5 takes the new filename in
 * its target field and blocks copying onto the source itself. See
 * `src/lib/file-operations/transfer/DETAILS.md` § "Single-item destinations".
 *
 * Fixture layout (at $CMDR_E2E_START_PATH): `left/` holds `file-a.txt`,
 * `file-b.txt`, `sub-dir/`, `bulk/`, `.hidden-file`; `right/` starts empty.
 */

import { waitBudget } from './wait-budget.js'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpCall, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import { waitForConflictCheck } from './conflict-helpers.js'
import {
  clickButtonByText,
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  executeViaCommandPalette,
  fileExistsInPane,
  getFixtureRoot,
  moveCursorToFile,
  pressKey,
  renameEditorValue,
  waitForTransferUiToSettle,
  TRANSFER_DIALOG,
  CTRL_OR_META,
} from './helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// Putting the shared `left/` + `right/` tree back is this spec's job: the
// post-test leak guard fails whoever leaves it dirty.
test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Duplicate in place', () => {
  test('⌘C then ⌘V in one pane lands "file-a (1).txt" beside the original', async ({ tauriPage }) => {
    // macOS only: every clipboard FILE command is `#[cfg(target_os = "macos")]`
    // (`src-tauri/src/commands/clipboard.rs`), and the others answer "not yet
    // supported on this platform", so ⌘C never reaches a pasteboard on Linux.
    // The E2E clipboard fake replaces the pasteboard INSIDE that macOS
    // implementation, so it doesn't make the commands exist elsewhere. Nothing
    // about duplicating is macOS-only: ⌘D, the Duplicate command, and F5 all
    // cover the same backend path on both platforms.
    test.skip(process.platform !== 'darwin', 'Clipboard file operations are implemented on macOS only.')

    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await pressKey(tauriPage, `${CTRL_OR_META}+c`)
    await expectAndDismissToast(tauriPage, 'Copied 1 item', { timeout: waitBudget(5000) })

    // Paste into the pane the file is already in. No conflict dialog may appear:
    // an item landing on itself is a request to duplicate it.
    await pressKey(tauriPage, `${CTRL_OR_META}+v`)

    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt')), { timeout: waitBudget(8000) })
      .toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
    await expect
      .poll(async () => fileExistsInPane(tauriPage, 'file-a (1).txt', 0), { timeout: waitBudget(5000) })
      .toBeTruthy()

    // The progress dialog is up until the operation ends, so both this wait and the
    // toast below are asking about a finished operation. It has to be the UI-side
    // wait: the backend's registry emptying says nothing about the frontend having
    // taken `write-complete` off the event bridge and closed the dialog (~0 ms idle,
    // 650 ms in a loaded lane).
    await waitForTransferUiToSettle(tauriPage)
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')

    // Paste is one of the two gestures that end a single-item duplicate in the
    // rename editor, seeded with the name the backend generated (read from the
    // operation journal, never recomputed here).
    await tauriPage.waitForSelector('.rename-input', 5000)
    await expect.poll(async () => renameEditorValue(tauriPage), { timeout: waitBudget(3000) }).toBe('file-a (1).txt')

    // Esc keeps the generated name: the copy stays exactly where the paste put it.
    await tauriPage.press('.rename-input', 'Escape')
    await expect
      .poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: waitBudget(5000) })
      .toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt'))).toBe(true)
  })

  test('⌘D duplicates the cursor item where it stands, with no dialog and no editor', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await pressKey(tauriPage, `${CTRL_OR_META}+d`)

    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt')), { timeout: waitBudget(8000) })
      .toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
    await expect
      .poll(async () => fileExistsInPane(tauriPage, 'file-a (1).txt', 0), { timeout: waitBudget(5000) })
      .toBeTruthy()
    // Neither wait above says the operation is OVER (the copy is on disk before the
    // closing flush, and the row comes from the pane's watcher), and the toast and
    // the dialog's close are both the completion's doing, on the frontend's turn
    // rather than the backend's. `waitForTransferUiToSettle` has the whole argument.
    await waitForTransferUiToSettle(tauriPage)
    // Wait out the completion toast BEFORE asking about the editor: the rename
    // follow-up would open just after it, so asking earlier would pass vacuously.
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')

    // Duplicate asks nothing: no destination to pick, and no name to type. Unlike
    // paste and F5, it must not leave the rename editor open: a second ⌘D has to
    // stamp out another copy rather than land in a text field.
    // NOT polled: "the editor never opened" is an absence, and a poll for absence
    // passes on its first read anyway.
    expect(await tauriPage.isVisible('.rename-input')).toBe(false)

    // And a second one really does stamp out another copy rather than typing into
    // an editor the first one left open. The cursor is placed explicitly: the new
    // row sorts BEFORE its source (a space beats a dot), so where the cursor lands
    // after the refresh is the pane's business, not this test's.
    expect(await moveCursorToFile(tauriPage, 'file-a.txt')).toBe(true)
    await pressKey(tauriPage, `${CTRL_OR_META}+d`)
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (2).txt')), { timeout: waitBudget(8000) })
      .toBeTruthy()
    await waitForTransferUiToSettle(tauriPage)
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')
  })

  test('the Duplicate command copies a whole selection at once, from the menu and the palette', async ({
    tauriPage,
  }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()
    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))

    await mcpCall('select', { pane: 'left', names: ['file-a.txt', 'file-b.txt'] })

    // The route the native menu bar and the right-click menu both take: Rust emits
    // `execute-command` with the id `menu_id_to_command` resolved.
    await dispatchMenuCommand(tauriPage, 'file.duplicate')

    await expect
      .poll(
        () =>
          fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt')) &&
          fs.existsSync(path.join(fixtureRoot, 'left', 'file-b (1).txt')),
        { timeout: waitBudget(8000) },
      )
      .toBeTruthy()
    // UI-side: the line below asks about the rename editor, which is frontend state,
    // so the wait before it has to reach the frontend too.
    await waitForTransferUiToSettle(tauriPage)
    await expectAndDismissToast(tauriPage, 'Copied 2 files.')
    expect(await tauriPage.isVisible('.rename-input')).toBe(false)

    // And the palette entry reaches the same command, on the item under the cursor.
    const found = await moveCursorToFile(tauriPage, 'file-b.txt')
    expect(found).toBe(true)
    await executeViaCommandPalette(tauriPage, 'Duplicate')

    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-b (2).txt')), { timeout: waitBudget(8000) })
      .toBeTruthy()
    await waitForTransferUiToSettle(tauriPage)
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')
  })

  test('F5 blocks the source path and copies a relative filename beside it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const leftDir = path.join(fixtureRoot, 'left')
    await mcpNavToPath('right', leftDir)

    const found = await moveCursorToFile(tauriPage, 'file-b.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    // An invalid source-identical target never starts a conflict check.
    await expect
      .poll(async () => tauriPage.isVisible(`${TRANSFER_DIALOG} .path-error`), { timeout: waitBudget(5000) })
      .toBeTruthy()
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .conflicts-summary`)).toBe(false)
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .conflict-policy`)).toBe(false)
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .path-error`)).toBe(true)
    expect(
      await tauriPage.evaluate<boolean>(`document.querySelector('${TRANSFER_DIALOG} .btn-primary').disabled`),
    ).toBe(true)

    const sourceContents = fs.readFileSync(path.join(leftDir, 'file-b.txt'), 'utf8')
    await tauriPage.fill(`${TRANSFER_DIALOG} input.text-field-control`, 'file-b copy.txt')
    await waitForConflictCheck(tauriPage)
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .path-error`)).toBe(false)

    await clickButtonByText(tauriPage, `${TRANSFER_DIALOG} .btn-primary`, 'Copy')

    // The progress dialog takes the setup dialog's place, so a transfer surface is up
    // continuously from here until the operation ends AND the frontend has closed it.
    // The file lands before the operation ends and proves it started, so the settle
    // wait can't pass vacuously here.
    await expect
      .poll(() => fs.existsSync(path.join(leftDir, 'file-b copy.txt')), { timeout: waitBudget(8000) })
      .toBeTruthy()
    await waitForTransferUiToSettle(tauriPage)

    expect(fs.readFileSync(path.join(leftDir, 'file-b.txt'), 'utf8')).toBe(sourceContents)
    expect(fs.readFileSync(path.join(leftDir, 'file-b copy.txt'), 'utf8')).toBe(sourceContents)
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')
    expect(await tauriPage.isVisible('.rename-input')).toBe(false)
  })
})
