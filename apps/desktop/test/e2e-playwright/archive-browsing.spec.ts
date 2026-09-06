/**
 * E2E tests for READING an archive: what Enter does, and what browsing one shows.
 *
 * Verifies the user-visible flows: pressing Enter on a `.zip` steps inside it
 * like a folder with a transparent path, navigating out exits the archive, a
 * real directory merely NAMED like a zip stays a plain folder, a file inside
 * previews and copies out, the Enter-behavior popup offers Browse | Open |
 * Configure per format, and the read-only formats (OOXML documents, tar.gz)
 * browse but refuse every write.
 *
 * Rewriting a zip in place (mkdir, rename, delete, paste, move-out, conflicts)
 * is `archive-editing.spec.ts`; the two share `archive-helpers.ts`.
 *
 * Fixture (at $CMDR_E2E_START_PATH, recreated per test):
 *   left/
 *     sample.zip            <- a real zip: inner.txt + nested/deep.txt
 *     sample.tar.gz         <- the same tree, read-only
 *     sample.docx           <- a real Word file (a zip of document parts)
 *     report.docx           <- a `.docx` in name only (plain text), for the Open default
 *     decoy.zip/            <- a real DIRECTORY named like a zip (marker.txt inside)
 *     file-a.txt, file-b.txt, sub-dir/, bulk/, ...
 *   right/                  <- empty
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import {
  ensureAppReady,
  getFixtureRoot,
  getFocusedPaneActiveTabPath,
  settleFocusedPaneOnLeft,
  moveCursorToFile,
  fileExistsInFocusedPane,
  openViewerWindow,
  closeScopedWindow,
  expectAndDismissToast,
  dismissOverlay,
  getOpenedPaths,
  clearOpenedPaths,
  TRANSFER_DIALOG,
  MKDIR_DIALOG,
} from './helpers.js'
import { ENTER_MENU, enterEntry, setArchiveEnterBehavior, settleOnFixtureLeft } from './archive-helpers.js'

import type { TauriPage } from '@srsholmes/tauri-playwright'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// Putting the shared `left/` + `right/` tree back is this spec's job: the
// post-test leak guard fails whoever leaves it dirty, and the restore is
// surgical, so it only rewrites what actually drifted.
test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Archive browsing', () => {
  // These tests browse INTO archives directly, so force the zip Enter behavior to
  // Browse (the default is Ask, which would pop the menu instead — that flow is
  // covered by the "Archive Enter-behavior menu" suite below).
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await settleOnFixtureLeft(tauriPage, 'sample.zip')
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
    await settleFocusedPaneOnLeft(tauriPage, `${zipPath}/nested`)
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
  // preview routes the archive-inner source through its `ArchiveVolume`
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
})

test.describe('Archive Enter-behavior menu', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await settleOnFixtureLeft(tauriPage, 'sample.zip')
    // The headline flow: zip set to Ask (the default), so Enter pops the menu.
    // `ooxml` is set EXPLICITLY rather than left alone: settings persist across
    // tests in one app instance, so the OOXML suite's `browse` would otherwise
    // leak in and turn "defaults to Open" into a browse. Stating the premise also
    // stops that test from silently depending on which suite ran before it.
    await setArchiveEnterBehavior({ zip: 'ask', bundle: 'ask', ooxml: 'open' })
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

// Document containers: an OOXML file (`.docx` / `.xlsx` / `.pptx` / `.jar` /
// `.apk`) IS a zip, so Browse steps inside and shows the parts a document is made
// of. It is READ-ONLY, and for a stronger reason than tar and 7z are: those have
// no mutator at all, while a `.docx` is a zip the mutator would happily rewrite.
// A user who wanders in to look around must not be able to walk out with a
// corrupt document, which is what makes offering Browse safe in the first place.
test.describe('Archive browsing — read-only OOXML documents', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await settleOnFixtureLeft(tauriPage, 'sample.docx')
    // The default is Open (a document is a document), so Browse has to be asked for.
    await setArchiveEnterBehavior({ ooxml: 'browse' })
  })

  test('pressing Enter on a .docx set to Browse steps inside and lists its parts', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const docxPath = `${getFixtureRoot()}/left/sample.docx`

    await enterEntry(tauriPage, 'sample.docx')

    // Transparent path, exactly like a zip: the tab keeps the parent drive's id.
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 5000 }).toBe(docxPath)
    // The parts a Word file is actually made of.
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'word'), { timeout: 5000 }).toBeTruthy()
    await expect
      .poll(async () => fileExistsInFocusedPane(tauriPage, '[Content_Types].xml'), { timeout: 5000 })
      .toBeTruthy()
  })

  test('creating a folder inside a .docx is refused, so a document can never be rewritten', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    await enterEntry(tauriPage, 'sample.docx')
    await expect.poll(async () => fileExistsInFocusedPane(tauriPage, 'word'), { timeout: 5000 }).toBeTruthy()

    // The read-only alert up front, never the mkdir dialog. The pane's capability
    // row refuses here, and `ensure_zip_writable` refuses in the backend too, so
    // this holds for an MCP or IPC caller that never sees a dialog at all.
    await tauriPage.keyboard.press('F7')
    await expect.poll(async () => tauriPage.isVisible('[data-dialog-id="alert"]'), { timeout: 5000 }).toBeTruthy()
    expect(await tauriPage.isVisible(MKDIR_DIALOG)).toBe(false)

    await dismissOverlay(tauriPage)
    expect(await fileExistsInFocusedPane(tauriPage, 'word')).toBe(true)
  })

  test('renaming a part inside a .docx is refused', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const docxPath = `${getFixtureRoot()}/left/sample.docx`
    const before = fs.readFileSync(docxPath)

    await enterEntry(tauriPage, 'sample.docx')
    await expect
      .poll(async () => fileExistsInFocusedPane(tauriPage, '[Content_Types].xml'), { timeout: 5000 })
      .toBeTruthy()

    const found = await moveCursorToFile(tauriPage, '[Content_Types].xml')
    expect(found).toBe(true)
    await tauriPage.keyboard.press('F2')

    // The refusal is LOUD, not a silent no-op: the read-only alert comes up and
    // no rename editor opens, so the user is told why rather than left pressing
    // a dead key.
    await expect.poll(async () => tauriPage.isVisible('[data-dialog-id="alert"]'), { timeout: 5000 }).toBeTruthy()
    expect(await tauriPage.isVisible('.rename-input')).toBe(false)

    await dismissOverlay(tauriPage)

    // The document's bytes are untouched — the assertion that actually matters.
    expect(fs.readFileSync(docxPath).equals(before)).toBe(true)
  })
})

// The read-only formats: a `.tar.gz` browses and extracts like a zip, but every
// mutation is refused (tar/7z are browse + extract only). `sample.tar.gz` carries
// the same `inner.txt` + `nested/deep.txt` as `sample.zip`.
test.describe('Archive browsing — read-only tar.gz', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await settleOnFixtureLeft(tauriPage, 'sample.tar.gz')
    // tar/7z ride the same `zip` Enter policy (backend `is_archive`), so Browse
    // steps into them too.
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
