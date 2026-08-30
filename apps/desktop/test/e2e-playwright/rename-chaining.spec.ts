/**
 * E2E for chaining the inline rename with the arrow keys: ArrowDown saves what's
 * in the editor and reopens it on the row below, ArrowUp on the row above.
 *
 * These run against the real backend on purpose. A chain's own renames re-sort
 * the directory under it, and the three listings involved (the backend's cache,
 * the pane's cursor, and the loaded window it renders from) each catch up on
 * their own schedule, which is a timing shape no unit test reproduces for free.
 * So every assertion that matters is made against DISK, by file content.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  ensureAppReady,
  expectedLeftPaneEntries,
  getFixtureRoot,
  moveCursorToFile,
  renameEditorValue,
  setRenameInput,
} from './helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// Both tests write their own rows into the shared `left/` tree; the restore is
// surgical, so it only takes those away again.
test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Chained rename', () => {
  test('an arrow carries the rename down three files, even when the first one re-sorts away', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()

    // Two hops, not one: only the second hop opens an editor while an earlier
    // chained save is still in flight, which is the crossing every session id
    // in this flow exists to prevent. The shared fixture stops at `file-b.txt`,
    // so the third row is this test's own; the leak guard's restore takes it
    // away again afterwards.
    fs.writeFileSync(path.join(fixtureRoot, 'left', 'file-c.txt'), 'chained rename fixture\n')
    // Every top-level row, not just the three this test types over: `ensureAppReady`'s
    // poll is an `every()`, so a narrower list is satisfied by a pane that is still
    // catching up on `recreateFixtures`' churn, and the chain then runs against a
    // listing the app is still rewriting underneath it.
    await ensureAppReady(tauriPage, { leftPane: expectedLeftPaneEntries(fixtureRoot) })

    expect(await moveCursorToFile(tauriPage, 'file-a.txt')).toBe(true)
    await tauriPage.keyboard.press('F2')
    await tauriPage.waitForSelector('.rename-input', 3000)

    // `z-…` sorts below the other two, so the renamed file leaves the row the
    // chain is standing on. The hop must still land on the file that WAS next.
    await setRenameInput(tauriPage, 'z-chained-a.txt')
    await tauriPage.press('.rename-input', 'ArrowDown')

    // The editor reopened on file-b.txt with its own name in it, ready to type over.
    await expect.poll(async () => renameEditorValue(tauriPage), { timeout: 3000 }).toBe('file-b.txt')

    // This one keeps its place in the sort, so the row below is `file-c.txt`
    // whether or not the re-sort has landed yet.
    await setRenameInput(tauriPage, 'file-b-chained.txt')
    await tauriPage.press('.rename-input', 'ArrowDown')

    await expect.poll(async () => renameEditorValue(tauriPage), { timeout: 3000 }).toBe('file-c.txt')

    await setRenameInput(tauriPage, 'z-chained-c.txt')
    await tauriPage.press('.rename-input', 'Enter')

    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 5000 }).toBeTruthy()

    // Each name landed on its own file; no save crossed over.
    await expect
      .poll(
        () =>
          fs.existsSync(path.join(fixtureRoot, 'left', 'z-chained-a.txt')) &&
          fs.existsSync(path.join(fixtureRoot, 'left', 'file-b-chained.txt')) &&
          fs.existsSync(path.join(fixtureRoot, 'left', 'z-chained-c.txt')),
        { timeout: 5000 },
      )
      .toBeTruthy()
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(false)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-b.txt'))).toBe(false)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-c.txt'))).toBe(false)
  })

  test('a chain held down through six files skips none of them, with every rename re-sorting away', async ({
    tauriPage,
  }) => {
    const fixtureRoot = getFixtureRoot()
    const hops = [1, 2, 3, 4, 5, 6]

    // `hop-*.txt` sorts as one run between `file-b.txt` and `report.docx`, and
    // each name they get sorts past every other row, so every step moves a row
    // from above the cursor to below it. That shifts the backend's listing under
    // a chain going the other way, while its `directory-diff` is still inside the
    // 50 ms coalescing window: the state a step that trusted an index reads a row
    // or two too far in, leaving files behind with their old names.
    for (const hop of hops) {
      fs.writeFileSync(path.join(fixtureRoot, 'left', `hop-${String(hop)}.txt`), `content of hop ${String(hop)}\n`)
    }
    await ensureAppReady(tauriPage, { leftPane: expectedLeftPaneEntries(fixtureRoot) })

    expect(await moveCursorToFile(tauriPage, 'hop-1.txt')).toBe(true)
    await tauriPage.keyboard.press('F2')
    await tauriPage.waitForSelector('.rename-input', 3000)

    // Each step waits only for the editor to land on the next file, never for a
    // rename to come back: the chain runs with the frontend a rename or two
    // behind the disk, which is the state the row-skip lived in. Reading the name
    // back also says the editor landed on the row it was supposed to.
    for (const hop of hops) {
      await expect.poll(async () => renameEditorValue(tauriPage), { timeout: 3000 }).toBe(`hop-${String(hop)}.txt`)
      await setRenameInput(tauriPage, `zz-hop-${String(hop)}.txt`)
      if (hop === hops.length) await tauriPage.press('.rename-input', 'Enter')
      else await tauriPage.press('.rename-input', 'ArrowDown')
    }

    await expect.poll(async () => !(await tauriPage.isVisible('.rename-input')), { timeout: 5000 }).toBeTruthy()

    // On disk, not in the UI: every file got the name typed for IT, and none was
    // flown past.
    await expect
      .poll(
        () =>
          fs
            .readdirSync(path.join(fixtureRoot, 'left'))
            .filter((name) => name.includes('hop-'))
            .sort(),
        { timeout: 5000 },
      )
      .toEqual(hops.map((hop) => `zz-hop-${String(hop)}.txt`))
    for (const hop of hops) {
      expect(fs.readFileSync(path.join(fixtureRoot, 'left', `zz-hop-${String(hop)}.txt`), 'utf8')).toBe(
        `content of hop ${String(hop)}\n`,
      )
      expect(fs.existsSync(path.join(fixtureRoot, 'left', `hop-${String(hop)}.txt`))).toBe(false)
    }
  })
})
