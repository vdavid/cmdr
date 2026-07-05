/**
 * E2E tests for archive browsing AND editing (zip-as-folder).
 *
 * Verifies the user-visible flows: pressing Enter on a `.zip` steps inside it
 * like a folder with a transparent path, navigating out exits the archive, a
 * real directory merely NAMED like a zip stays a plain folder, a file inside the
 * zip previews and copies out, and — now that zips are writable — creating,
 * renaming, deleting, pasting into, and moving out of a zip all run the managed
 * archive-edit flow, with a permanent (no-Trash) delete confirm and an intact
 * original when a paste is cancelled.
 *
 * Fixture (at $CMDR_E2E_START_PATH, recreated per test):
 *   left/
 *     sample.zip            <- a real zip: inner.txt + nested/deep.txt
 *     decoy.zip/            <- a real DIRECTORY named like a zip (marker.txt inside)
 *     file-a.txt, file-b.txt, sub-dir/, bulk/, ...
 *   right/                  <- empty
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpReadResource, mcpCall } from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  getFixtureRoot,
  moveCursorToFile,
  fileExistsInFocusedPane,
  openViewerWindow,
  closeScopedWindow,
  expectAndDismissToast,
  dismissOverlay,
  TRANSFER_DIALOG,
  MKDIR_DIALOG,
} from './helpers.js'

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const DELETE_DIALOG = '[data-dialog-id="delete-confirmation"]'
const TRANSFER_PROGRESS = '[data-dialog-id="transfer-progress"]'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

/**
 * Reads the focused pane's active-tab path from the MCP `cmdr://state` resource.
 * The active-tab line carries the path in parentheses; inside an archive this is
 * the transparent `…/sample.zip[/inner]` path (the tab keeps the parent-drive id,
 * MCP reports the full path). Mirrors `go-to-path.spec.ts`.
 */
async function getFocusedPaneActiveTabPath(): Promise<string | null> {
  const state = await mcpReadResource('cmdr://state?compact=true')
  const focusedMatch = /^focused:\s*(left|right)/m.exec(state)
  if (focusedMatch === null) return null
  const pane = focusedMatch[1]
  const marker = `\n${pane}:\n`
  const idx = state.indexOf(marker)
  if (idx === -1) return null
  const block = state.slice(idx + marker.length)
  const endIdx = block.search(/\n[a-z]/)
  const scoped = endIdx === -1 ? block : block.slice(0, endIdx)
  const m = /^\s+- i:\d+ id:\S+ \[active\][^\n]*\(([^)\n]+)\)\s*$/m.exec(scoped)
  return m?.[1] ?? null
}

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

/** Moves the cursor to `name` in the focused pane and presses Enter to open it. */
async function enterEntry(tauriPage: PageLike, name: string): Promise<void> {
  const found = await moveCursorToFile(tauriPage, name)
  expect(found, `entry "${name}" should be in the focused pane`).toBe(true)
  await tauriPage.keyboard.press('Enter')
}

/** Navigate a pane to a path via the same `mcp-nav-to-path` event the MCP server uses. */
async function navigatePaneTo(tauriPage: PageLike, pane: 'left' | 'right', targetPath: string): Promise<void> {
  await tauriPage.evaluate(`(function () {
        window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: ${JSON.stringify(pane)}, path: ${JSON.stringify(targetPath)} }
        });
    })()`)
}

const ENTER_MENU = '.menu-content'

/**
 * Sets the per-format Enter behavior (the `behavior.archiveEnterBehavior` pinned-
 * shape JSON) through the same MCP `set_setting` path the UI uses. `set_setting`
 * round-trips, so the setting is live by the time this resolves.
 */
async function setArchiveEnterBehavior(behavior: Record<string, string>): Promise<void> {
  await mcpCall('set_setting', { id: 'behavior.archiveEnterBehavior', value: JSON.stringify(behavior) })
}

/** The paths `open_path` recorded in the E2E build (LaunchServices is mocked, never launched). */
async function getOpenedPaths(tauriPage: PageLike): Promise<string[]> {
  return tauriPage.evaluate<string[]>(
    `(async function(){ return await window.__TAURI_INTERNALS__.invoke('e2e_opened_paths'); })()`,
  )
}

/** Resets the recorded open requests (per-test isolation). */
async function clearOpenedPaths(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('e2e_clear_opened_paths')`)
}

test.describe('Archive browsing', () => {
  // These tests browse INTO archives directly, so force the zip Enter behavior to
  // Browse (the default is Ask, which would pop the menu instead — that flow is
  // covered by the "Archive Enter-behavior menu" suite below).
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    // Return the focused pane to `left/` and confirm it landed: a prior test may have
    // left it browsing INSIDE the archive, and `ensureAppReady` doesn't reliably back
    // out of an archive volume, so start each test from a known directory.
    await navigatePaneTo(tauriPage, 'left', `${getFixtureRoot()}/left`)
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(`${getFixtureRoot()}/left`)
    // Wait for the listing to actually repopulate before a test reads it. The top-level
    // beforeEach wipes and rewrites `left/`, so the path can already read `left/` (a prior
    // test ended here, making the nav a no-op) while the pane still shows the mid-refresh
    // empty view. Poll for a known fixture entry so the file-watcher has caught up.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'sample.zip'), { timeout: 5000 }).toBeTruthy()
    await setArchiveEnterBehavior({ zip: 'browse', bundle: 'browse' })
  })

  test('pressing Enter on a zip lists its inner entries with a transparent path', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const zipPath = `${getFixtureRoot()}/left/sample.zip`

    await enterEntry(tauriPage, 'sample.zip')

    // The pane is now INSIDE the archive; the path is the transparent zip path
    // (no scheme prefix), and the parent drive is still the tab's volume.
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(zipPath)
    // The inner entries are listed like a folder.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'nested'), { timeout: 5000 }).toBeTruthy()
  })

  test('navigating into a nested archive dir and back out exits the archive', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const leftDir = `${getFixtureRoot()}/left`
    const zipPath = `${leftDir}/sample.zip`

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(zipPath)

    // Into the nested dir inside the archive.
    await enterEntry(tauriPage, 'nested')
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(`${zipPath}/nested`)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'deep.txt'), { timeout: 5000 }).toBeTruthy()

    // Backspace bubbles up to the archive root...
    await tauriPage.keyboard.press('Backspace')
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(zipPath)

    // ...and again out of the archive entirely, to its containing folder.
    await tauriPage.keyboard.press('Backspace')
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(leftDir)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'sample.zip'), { timeout: 5000 }).toBeTruthy()
  })

  test('a real directory named like a zip enters as a plain folder', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const decoyPath = `${getFixtureRoot()}/left/decoy.zip`

    await enterEntry(tauriPage, 'decoy.zip')

    // The boundary check must lose to normal directory navigation: `decoy.zip` is
    // a real directory, so we enter it as a plain folder and see its real contents.
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(decoyPath)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'marker.txt'), { timeout: 5000 }).toBeTruthy()
  })

  test('previewing a text file inside the archive shows its content', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const innerFile = `${getFixtureRoot()}/left/sample.zip/inner.txt`

    // The viewer opens an archive-inner path via bounded temp-extract.
    const viewer = await openViewerWindow(tauriPage as TauriPage, innerFile)
    const viewerLabel = viewer.targetWindow
    if (!viewerLabel) throw new Error('Scoped viewer page has no targetWindow label')
    try {
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)
      expect(await viewer.isVisible('.file-content')).toBe(true)
      const content = await viewer.textContent('.file-content')
      expect(content).toContain('hello from inside the archive')
      const statusText = await viewer.textContent('.status-bar')
      expect(statusText).toContain('inner.txt')
    } finally {
      await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
    }
  })

  test('pressing Enter on a text file inside the archive opens the viewer (not a dead-end)', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const main = tauriPage as TauriPage

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    // Enter on a non-archive file inside the zip routes to the VIEWER (temp-extract).
    // The OS default-app open would be a silent no-op on the inner path, so a new
    // viewer window opening is the proof the dead-end is gone.
    const before = new Set((await main.listWindows()).map((w) => w.label).filter((l) => l.startsWith('viewer-')))
    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('Enter')

    const viewer = await main.waitForWindow((w) => w.label.startsWith('viewer-') && !before.has(w.label), {
      timeout: 10000,
    })
    const viewerLabel = viewer.targetWindow
    if (!viewerLabel) throw new Error('Scoped viewer page has no targetWindow label')
    try {
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)
      expect(await viewer.textContent('.file-content')).toContain('hello from inside the archive')
    } finally {
      await closeScopedWindow(main, viewer, viewerLabel)
    }
  })

  // Extract-out: copy a file from inside the archive to the local pane. The scan
  // preview now routes the archive-inner source through its `ArchiveVolume`
  // (`scan_preview_source_volume`), so the cached preview has the real file count
  // instead of the 0-file `std::fs` result that stalled the copy at "0 files".
  test('copying a file out of the archive extracts it to the other pane', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    // F5 copies the cursored entry from the archive (source) to the right pane.
    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    // The extracted file lands on disk in the right pane's folder.
    await expect.poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'inner.txt')), { timeout: 5000 }).toBeTruthy()
    expect(fs.readFileSync(path.join(fixtureRoot, 'right', 'inner.txt'), 'utf8')).toContain(
      'hello from inside the archive',
    )

    // The success toast is part of the contract; asserting it also clears it
    // (the global afterEach fails any test that leaks a toast).
    await expectAndDismissToast(tauriPage, 'Copied 1 file')
  })

  test('copying the zip FILE itself copies the whole archive, not its contents', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Do NOT enter the archive: F5 on the `.zip` file itself must copy the whole
    // file (a `.zip` is a regular file), not route into it and scan its contents.
    const found = await moveCursorToFile(tauriPage, 'sample.zip')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    // The whole zip lands in the right pane, byte-identical to the source file.
    const dest = path.join(fixtureRoot, 'right', 'sample.zip')
    await expect.poll(() => fs.existsSync(dest), { timeout: 5000 }).toBeTruthy()
    const srcBytes = fs.readFileSync(path.join(fixtureRoot, 'left', 'sample.zip'))
    expect(fs.readFileSync(dest).equals(srcBytes)).toBe(true)

    await expectAndDismissToast(tauriPage, 'Copied 1 file')
  })

  test('creating a folder inside the archive adds it and shows it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    // F7 inside a zip now runs the real managed archive-edit flow (no refusal).
    const folderName = `zip-folder-${String(Date.now())}`
    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(MKDIR_DIALOG, 5000)
    await tauriPage.waitForSelector(`${MKDIR_DIALOG} .name-input`, 3000)
    await tauriPage.fill(`${MKDIR_DIALOG} .name-input`, folderName)
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
    // archive warning and no Trash/Delete toggle.
    await tauriPage.keyboard.press('F8')
    await tauriPage.waitForSelector(DELETE_DIALOG, 5000)
    const bannerText = await tauriPage.textContent(`${DELETE_DIALOG} .warning-banner`)
    expect(bannerText).toContain('no trash inside an archive')
    expect(await tauriPage.isVisible(`${DELETE_DIALOG} .operation-toggle`)).toBe(false)

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

    // Right pane inside the zip (the copy destination); left focused on a real file.
    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

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

  test('cancelling a paste into the archive leaves the zip contents intact', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // A large source file gives a window to cancel mid-write. Create it directly
    // (the shared bulk cache isn't populated for a single manual instance).
    const bigName = 'big-to-cancel.dat'
    fs.writeFileSync(path.join(fixtureRoot, 'left', bigName), Buffer.alloc(24 * 1024 * 1024, 7))

    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

    // Left pane stays at `left/` (from the describe beforeEach); cursor the big file.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, bigName), { timeout: 5000 }).toBeTruthy()
    const found = await moveCursorToFile(tauriPage, bigName)
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    // Cancel as soon as the progress dialog appears (temp+rename means the original
    // is untouched until the final atomic rename, so cancel can't corrupt it).
    await tauriPage.waitForSelector(TRANSFER_PROGRESS, 5000)
    await tauriPage
      .waitForSelector(`${TRANSFER_PROGRESS} .btn-cancel, ${TRANSFER_PROGRESS} button.cancel`, 3000)
      .catch(() => {})
    await tauriPage.evaluate(`(function(){
        var dlg = document.querySelector('${TRANSFER_PROGRESS}');
        var btns = dlg ? Array.prototype.slice.call(dlg.querySelectorAll('button')) : [];
        var cancel = btns.find(function(b){ return /cancel/i.test((b.textContent||'')); });
        if (cancel) cancel.click();
    })()`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 20000 }).toBeTruthy()
    await clearAllToasts(tauriPage)

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

    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?compact=true')).includes('sample.zip'), { timeout: 5000 })
      .toBeTruthy()

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

test.describe('Archive Enter-behavior menu', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    // Return the focused pane to `left/` and confirm it landed: a prior test may have
    // left it browsing INSIDE the archive, and `ensureAppReady` doesn't reliably back
    // out of an archive volume, so start each menu test from a known directory.
    await navigatePaneTo(tauriPage, 'left', `${getFixtureRoot()}/left`)
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(`${getFixtureRoot()}/left`)
    // Wait for the listing to actually repopulate before a test reads it. The top-level
    // beforeEach wipes and rewrites `left/`, so the path can already read `left/` (a prior
    // test ended here, making the nav a no-op) while the pane still shows the mid-refresh
    // empty view. Poll for a known fixture entry so the file-watcher has caught up.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'sample.zip'), { timeout: 5000 }).toBeTruthy()
    // The headline flow: zip set to Ask (the default), so Enter pops the menu.
    await setArchiveEnterBehavior({ zip: 'ask', bundle: 'ask' })
    await clearOpenedPaths(tauriPage)
  })

  test('Enter on a zip set to Ask shows the menu; Browse steps inside', async ({ tauriPage }) => {
    const zipPath = `${getFixtureRoot()}/left/sample.zip`

    await enterEntry(tauriPage, 'sample.zip')

    // The popup appears instead of navigating.
    await tauriPage.waitForSelector(ENTER_MENU, 5000)
    expect(await getFocusedPaneActiveTabPath()).toBe(`${getFixtureRoot()}/left`)

    // Browse is highlighted on open, so Enter picks it and steps inside.
    await tauriPage.keyboard.press('Enter')
    await expect.poll(async () => !(await tauriPage.isVisible(ENTER_MENU)), { timeout: 3000 }).toBeTruthy()
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(zipPath)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
  })

  test('Enter then Down then Enter picks Open, launching the zip in the default app', async ({ tauriPage }) => {
    const zipPath = `${getFixtureRoot()}/left/sample.zip`

    await enterEntry(tauriPage, 'sample.zip')
    await tauriPage.waitForSelector(ENTER_MENU, 5000)

    // Browse is highlighted on open; ArrowDown moves to Open. Wait for the highlight
    // to actually land on Open before selecting (probing the state, not sleeping) so
    // Enter can't race ahead of the arrow.
    await tauriPage.keyboard.press('ArrowDown')
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `(function(){ var el = document.querySelector('.menu-item.is-highlighted'); return !!el && (el.textContent || '').indexOf('Open') !== -1; })()`,
          ),
        { timeout: 2000 },
      )
      .toBeTruthy()
    await tauriPage.keyboard.press('Enter')
    await expect.poll(async () => !(await tauriPage.isVisible(ENTER_MENU)), { timeout: 3000 }).toBeTruthy()

    // Open hands the `.zip` file itself to LaunchServices (mocked in E2E), and
    // does NOT browse into it — the pane stays put.
    await expect.poll(async () => getOpenedPaths(tauriPage), { timeout: 5000 }).toContain(zipPath)
    expect(await getFocusedPaneActiveTabPath()).toBe(`${getFixtureRoot()}/left`)
  })

  test('a zip set to Browse skips the menu and enters directly', async ({ tauriPage }) => {
    const zipPath = `${getFixtureRoot()}/left/sample.zip`
    await setArchiveEnterBehavior({ zip: 'browse', bundle: 'ask' })

    await enterEntry(tauriPage, 'sample.zip')

    // No popup: it steps straight inside.
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(zipPath)
    expect(await tauriPage.isVisible(ENTER_MENU)).toBe(false)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
  })

  test('a .docx defaults to Open with no menu', async ({ tauriPage }) => {
    const docxPath = `${getFixtureRoot()}/left/report.docx`

    await enterEntry(tauriPage, 'report.docx')

    // Document packages default to Open, so there's no popup — it opens directly.
    await expect.poll(async () => getOpenedPaths(tauriPage), { timeout: 5000 }).toContain(docxPath)
    expect(await tauriPage.isVisible(ENTER_MENU)).toBe(false)
  })

  test('Configure deep-links to the Archives settings section', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage

    await enterEntry(tauriPage, 'sample.zip')
    await tauriPage.waitForSelector(ENTER_MENU, 5000)
    // Browse → Open → Configure: two Downs land on Configure, Enter selects it.
    await tauriPage.keyboard.press('ArrowDown')
    await tauriPage.keyboard.press('ArrowDown')
    await tauriPage.keyboard.press('Enter')

    // The settings window (label `settings`) opens, deep-linked to Behavior > Archives.
    const settings = await main.waitForWindow((w) => w.label === 'settings', { timeout: 10000 })
    const settingsLabel = settings.targetWindow
    if (!settingsLabel) throw new Error('Scoped settings page has no targetWindow label')
    try {
      await settings.waitForSelector('[data-section-id="behavior-archives"]', 10000)
    } finally {
      await closeScopedWindow(main, settings, settingsLabel)
    }
  })
})

// The read-only formats: a `.tar.gz` browses and extracts like a zip, but every
// mutation is refused (tar/7z are browse + extract only). `sample.tar.gz` carries
// the same `inner.txt` + `nested/deep.txt` as `sample.zip`.
test.describe('Archive browsing — read-only tar.gz', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await navigatePaneTo(tauriPage, 'left', `${getFixtureRoot()}/left`)
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(`${getFixtureRoot()}/left`)
    // tar/7z ride the same `zip` Enter policy (backend `is_archive`), so Browse
    // steps into them too. Wait for the fixture so the watcher has caught up.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'sample.tar.gz'), { timeout: 5000 }).toBeTruthy()
    await setArchiveEnterBehavior({ zip: 'browse', bundle: 'browse' })
  })

  test('pressing Enter on a tar.gz lists its inner entries with a transparent path', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const archivePath = `${getFixtureRoot()}/left/sample.tar.gz`

    await enterEntry(tauriPage, 'sample.tar.gz')

    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(archivePath)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'nested'), { timeout: 5000 }).toBeTruthy()
  })

  test('copying a file out of the tar.gz extracts it to the other pane', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await enterEntry(tauriPage, 'sample.tar.gz')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    const found = await moveCursorToFile(tauriPage, 'inner.txt')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
    await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
    await expect.poll(async () => !(await tauriPage.isVisible('.modal-overlay')), { timeout: 5000 }).toBeTruthy()

    await expect.poll(() => fs.existsSync(path.join(fixtureRoot, 'right', 'inner.txt')), { timeout: 5000 }).toBeTruthy()
    expect(fs.readFileSync(path.join(fixtureRoot, 'right', 'inner.txt'), 'utf8')).toContain(
      'hello from inside the archive',
    )
    await expectAndDismissToast(tauriPage, 'Copied 1 file')
  })

  test('creating a folder inside the tar.gz is refused (read-only)', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.tar.gz')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    // F7 must surface the read-only-archive alert up front, NOT the mkdir dialog:
    // tar/7z can't be edited (only zip is writable).
    await tauriPage.keyboard.press('F7')
    await expect.poll(async () => tauriPage.isVisible('[data-dialog-id="alert"]'), { timeout: 5000 }).toBeTruthy()
    expect(await tauriPage.isVisible(MKDIR_DIALOG)).toBe(false)

    const alertText = await tauriPage.evaluate<string>(`(function() {
            var msg = document.querySelector('[data-dialog-id="alert"] .message, [data-dialog-id="alert"] #alert-dialog-message');
            return msg ? msg.textContent : '';
        })()`)
    expect(alertText.toLowerCase()).toContain('zip archives can be edited')

    await dismissOverlay(tauriPage)

    // Nothing was written into the archive.
    expect(await fileExistsInFocusedPane(tauriPage, 'inner.txt')).toBe(true)
  })
})
