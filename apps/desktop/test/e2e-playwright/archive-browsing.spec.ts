/**
 * E2E tests for archive browsing (zip-as-folder), milestone M1b.
 *
 * Verifies the user-visible flows: pressing Enter on a `.zip` steps inside it
 * like a folder with a transparent path, navigating out exits the archive, a
 * real directory merely NAMED like a zip stays a plain folder, a file inside the
 * zip previews and copies out, and write actions inside the archive are refused
 * with friendly copy (archives are read-only in this phase).
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
import { ensureMcpClient, mcpReadResource } from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  getFixtureRoot,
  moveCursorToFile,
  fileExistsInFocusedPane,
  openViewerWindow,
  closeScopedWindow,
  TRANSFER_DIALOG,
  MKDIR_DIALOG,
} from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const ALERT_DIALOG = '[data-dialog-id="alert"]'

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

/** Moves the cursor to `name` in the focused pane and presses Enter to open it. */
async function enterEntry(tauriPage: PageLike, name: string): Promise<void> {
  const found = await moveCursorToFile(tauriPage, name)
  expect(found, `entry "${name}" should be in the focused pane`).toBe(true)
  await tauriPage.keyboard.press('Enter')
}

/** Reads the alert dialog's title + message (empty strings when closed). */
async function readAlert(tauriPage: PageLike): Promise<{ title: string; message: string }> {
  return tauriPage.evaluate<{ title: string; message: string }>(`(function(){
      var root = document.querySelector('${ALERT_DIALOG}');
      if (!root) return { title: '', message: '' };
      var titleEl = root.querySelector('h2, .modal-title');
      var msgEl = root.querySelector('.message, #alert-dialog-message');
      return {
          title: titleEl ? (titleEl.textContent || '').trim() : '',
          message: msgEl ? (msgEl.textContent || '').trim() : '',
      };
  })()`)
}

/** Dismisses the alert dialog by clicking its button. */
async function dismissAlert(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
      var btn = document.querySelector('${ALERT_DIALOG} button');
      if (btn) btn.click();
  })()`)
  await expect.poll(async () => !(await tauriPage.isVisible(ALERT_DIALOG)), { timeout: 3000 }).toBeTruthy()
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

test.describe('Archive browsing (M1b)', () => {
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
  })

  test('creating a folder inside the archive is refused with the archive alert', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.zip')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()

    // F7 (new folder) inside the archive is refused frontend-side with the archive
    // read-only alert — no new-folder dialog opens.
    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(ALERT_DIALOG, 5000)
    expect(await tauriPage.isVisible(MKDIR_DIALOG)).toBe(false)
    const alert = await readAlert(tauriPage)
    expect(alert.title).toBe('Archives are read-only')
    expect(alert.message).toContain("Creating folders inside them isn't possible yet")
    await dismissAlert(tauriPage)
  })

  test('pasting into the archive is refused with the archive alert', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Put the RIGHT pane inside the archive (it becomes the copy destination),
    // keep the LEFT pane focused on a real file.
    await navigatePaneTo(tauriPage, 'right', `${fixtureRoot}/left/sample.zip`)
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state?compact=true')
          return state.includes('sample.zip')
        },
        { timeout: 5000 },
      )
      .toBeTruthy()

    const found = await moveCursorToFile(tauriPage, 'file-a.txt')
    expect(found).toBe(true)

    // F5 targets the opposite (right) pane, which is inside the archive → refused
    // with the archive alert, no transfer dialog.
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(ALERT_DIALOG, 5000)
    expect(await tauriPage.isVisible(TRANSFER_DIALOG)).toBe(false)
    const alert = await readAlert(tauriPage)
    expect(alert.title).toBe('Archives are read-only')
    expect(alert.message).toContain('copying into one')
    await dismissAlert(tauriPage)
  })
})
