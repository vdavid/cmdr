/**
 * E2E tests for archive browsing (zip-as-folder), read-only phase.
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
import { ensureMcpClient, mcpReadResource, mcpCall } from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  getFixtureRoot,
  moveCursorToFile,
  fileExistsInFocusedPane,
  openViewerWindow,
  closeScopedWindow,
  expectAndDismissToast,
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

/** Clicks the Enter-menu row whose visible label contains `substring`. */
async function clickEnterMenuItem(tauriPage: PageLike, substring: string): Promise<void> {
  await tauriPage.evaluate(`(function(){
      var items = Array.from(document.querySelectorAll('${ENTER_MENU} .menu-item'));
      var match = items.find(function(el){ return (el.textContent || '').indexOf(${JSON.stringify(substring)}) !== -1; });
      if (match) match.click();
  })()`)
}

test.describe('Archive browsing', () => {
  // These tests browse INTO archives directly, so force the zip Enter behavior to
  // Browse (the default is Ask, which would pop the menu instead — that flow is
  // covered by the "Archive Enter-behavior menu" suite below).
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
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

test.describe('Archive Enter-behavior menu', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
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

    // Browse steps inside the archive like a folder.
    await clickEnterMenuItem(tauriPage, 'Browse')
    await expect.poll(async () => !(await tauriPage.isVisible(ENTER_MENU)), { timeout: 3000 }).toBeTruthy()
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(zipPath)
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'inner.txt'), { timeout: 5000 }).toBeTruthy()
  })

  test('Enter then Down then Enter picks Open, launching the zip in the default app', async ({ tauriPage }) => {
    const zipPath = `${getFixtureRoot()}/left/sample.zip`

    await enterEntry(tauriPage, 'sample.zip')
    await tauriPage.waitForSelector(ENTER_MENU, 5000)

    // Browse is highlighted on open; ArrowDown moves to Open, Enter selects it.
    await tauriPage.keyboard.press('ArrowDown')
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
    await clickEnterMenuItem(tauriPage, 'Configure')

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
