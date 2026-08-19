/**
 * E2E for duplicating in place: a copy that lands in the folder its source
 * already lives in.
 *
 * Four gestures reach it and they don't behave alike, which is the whole point
 * of a spec of its own: ⌘V and F5 end a single-item duplicate in the inline
 * rename editor, while the Duplicate command (⌘D, the menu, the palette) asks
 * nothing at all. `src/lib/file-operations/transfer/DETAILS.md` § "Only paste
 * and F5 end a duplicate in the rename editor".
 *
 * Fixture layout (at $CMDR_E2E_START_PATH): `left/` holds `file-a.txt`,
 * `file-b.txt`, `sub-dir/`, `bulk/`, `.hidden-file`; `right/` starts empty.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpCall, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  executeViaCommandPalette,
  fileExistsInPane,
  getFixtureRoot,
  moveCursorToFile,
  pressKey,
  renameEditorValue,
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
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await pressKey(tauriPage, `${CTRL_OR_META}+c`)
    await expectAndDismissToast(tauriPage, 'Copied 1 item', { timeout: 5000 })

    // Paste into the pane the file is already in. No conflict dialog may appear:
    // an item landing on itself is a request to duplicate it.
    await pressKey(tauriPage, `${CTRL_OR_META}+v`)

    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt')), { timeout: 8000 })
      .toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
    await expect.poll(async () => fileExistsInPane(tauriPage, 'file-a (1).txt', 0), { timeout: 5000 }).toBeTruthy()

    expect(await tauriPage.isVisible('.modal-overlay')).toBe(false)
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')

    // Paste is one of the two gestures that end a single-item duplicate in the
    // rename editor, seeded with the name the backend generated (read from the
    // operation journal, never recomputed here).
    await tauriPage.waitForSelector('.rename-input', 5000)
    await expect.poll(async () => renameEditorValue(tauriPage), { timeout: 3000 }).toBe('file-a (1).txt')

    // Esc keeps the generated name: the copy stays exactly where the paste put it.
    await tauriPage.press('.rename-input', 'Escape')
    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 5000 }).toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt'))).toBe(true)
  })

  test('⌘D duplicates the cursor item where it stands, with no dialog and no editor', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    await pressKey(tauriPage, `${CTRL_OR_META}+d`)

    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (1).txt')), { timeout: 8000 })
      .toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
    await expect.poll(async () => fileExistsInPane(tauriPage, 'file-a (1).txt', 0), { timeout: 5000 }).toBeTruthy()
    // Wait out the completion toast BEFORE asking about the editor: the rename
    // follow-up would open just after it, so asking earlier would pass vacuously.
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')

    // Duplicate asks nothing: no destination to pick, and no name to type. Unlike
    // paste and F5, it must not leave the rename editor open: a second ⌘D has to
    // stamp out another copy rather than land in a text field.
    expect(await tauriPage.isVisible('.modal-overlay')).toBe(false)
    expect(await tauriPage.isVisible('.rename-input')).toBe(false)

    // And a second one really does stamp out another copy rather than typing into
    // an editor the first one left open. The cursor is placed explicitly: the new
    // row sorts BEFORE its source (a space beats a dot), so where the cursor lands
    // after the refresh is the pane's business, not this test's.
    expect(await moveCursorToFile(tauriPage, 'file-a.txt')).toBe(true)
    await pressKey(tauriPage, `${CTRL_OR_META}+d`)
    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-a (2).txt')), { timeout: 8000 })
      .toBeTruthy()
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
        { timeout: 8000 },
      )
      .toBeTruthy()
    await expectAndDismissToast(tauriPage, 'Copied 2 files.')
    expect(await tauriPage.isVisible('.rename-input')).toBe(false)

    // And the palette entry reaches the same command, on the item under the cursor.
    const found = await moveCursorToFile(tauriPage, 'file-b.txt')
    expect(found).toBe(true)
    await executeViaCommandPalette(tauriPage, 'Duplicate')

    await expect
      .poll(() => fs.existsSync(path.join(fixtureRoot, 'left', 'file-b (2).txt')), { timeout: 8000 })
      .toBeTruthy()
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')
  })

  test('F5 with both panes on one folder reports no conflict and asks no policy', async ({ tauriPage }) => {
    // The gesture that used to be worst: the dialog's conflict check is a
    // destination listing matched by NAME, so with both panes on `left/` it saw
    // the source itself sitting at the destination and announced a conflict,
    // showed the overwrite/skip/rename radios, and sent the backend a pre-known
    // conflict naming the source. Both panes on one folder is the only way F5
    // reaches a same-folder copy: the check runs once, against the destination
    // the dialog opened with.
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const leftDir = path.join(fixtureRoot, 'left')
    await mcpNavToPath('right', leftDir)

    const found = await moveCursorToFile(tauriPage, 'file-b.txt')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    await expect
      .poll(async () => !(await tauriPage.isVisible(`${TRANSFER_DIALOG} .conflicts-checking`)), { timeout: 10000 })
      .toBeTruthy()
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .conflicts-summary`)).toBe(false)
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .conflict-policy`)).toBe(false)
    expect(await tauriPage.isVisible(`${TRANSFER_DIALOG} .path-error`)).toBe(false)

    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    await expect.poll(() => fs.existsSync(path.join(leftDir, 'file-b (1).txt')), { timeout: 8000 }).toBeTruthy()
    expect(fs.existsSync(path.join(leftDir, 'file-b.txt'))).toBe(true)
    await expectAndDismissToast(tauriPage, 'Copied 1 file.')

    // F5 is the other gesture that opts in, and the editor opens in the pane the
    // user is looking at rather than the one the dialog called the destination.
    await tauriPage.waitForSelector('.rename-input', 5000)
    await expect.poll(async () => renameEditorValue(tauriPage), { timeout: 3000 }).toBe('file-b (1).txt')

    await tauriPage.press('.rename-input', 'Escape')
    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 5000 }).toBeTruthy()
    expect(fs.existsSync(path.join(leftDir, 'file-b (1).txt'))).toBe(true)
  })
})
