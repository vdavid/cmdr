/**
 * E2E tests for conflict resolution edge cases and special file types.
 *
 * Covers: copy rollback, sequential copy conflicts, single-file overwrite,
 * symlink conflicts, and type mismatch conflicts (file vs directory).
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  getFixtureRoot,
  moveCursorToFile,
  pollUntil,
  sleep,
  TRANSFER_DIALOG,
} from './helpers.js'
import {
  createSymlinkFixture,
  createTypeMismatchFixture,
  clearFixtureDirs,
  writeFile,
  readFile,
  fileExists,
  selectAll,
  waitForConflictPolicy,
  selectConflictPolicy,
  clickTransferStart,
  waitForDialogsToClose,
} from './conflict-helpers.js'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.describe('Cancel and rollback', () => {
  test('Cancel copy mid-operation rolls back partial files', async ({ tauriPage }) => {
    // Strategy: subscribe to `write-progress` events from the webview, wait
    // for one with `filesDone >= 1 && filesDone < filesTotal` (= "mid-copy,
    // backend has acknowledged at least one file done"), then click Rollback.
    //
    // The old approach polled `fs.existsSync(file-0) && !fs.existsSync(file-3)`
    // every 50 ms. On macOS APFS with `copyfile(3) COPYFILE_CLONE` each
    // per-file copy is near-instant, so the 200 ms × 5-file throttle creates
    // only a ~600 ms window where the FS-state assertion holds. Under load
    // (parallel shards) the polling sometimes never caught the window and the
    // test failed with `Received: false`. The progress event fires the moment
    // the backend records a file committed, before any wall-clock window
    // exists for the polling loop to miss.
    const fixtureRoot = getFixtureRoot()
    recreateFixtures(fixtureRoot)
    const partialLeft = path.join(fixtureRoot, 'left', 'partial')
    fs.mkdirSync(partialLeft, { recursive: true })
    for (let i = 0; i < 5; i++) {
      fs.writeFileSync(path.join(partialLeft, `file-${String(i)}.txt`), 'x'.repeat(1024))
    }

    await ensureAppReady(tauriPage)

    // Per-file throttle still kicks in so the copy can't finish before we
    // subscribe and click. 200 ms gives the producer side plenty of room.
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: 200 })`)

    try {
      // Subscribe to `write-progress` BEFORE starting the copy. Uses Tauri's
      // internal `transformCallback` to register a handler ID, then calls
      // `plugin:event|listen` directly (same path `@tauri-apps/api/event`'s
      // `listen()` uses, just inlined so the test doesn't need the JS API
      // loaded in the webview — `withGlobalTauri` is false in prod builds).
      // We store events on `window.__cancelCopyTestEvents` and the test reads
      // them via `evaluate`.
      await tauriPage.evaluate(`(async function() {
        window.__cancelCopyTestEvents = [];
        const handler = (event) => { window.__cancelCopyTestEvents.push(event.payload); };
        const handlerId = window.__TAURI_INTERNALS__.transformCallback(handler);
        window.__cancelCopyTestEventId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
          event: 'write-progress',
          target: { kind: 'Any' },
          handler: handlerId,
        });
      })()`)

      const found = await moveCursorToFile(tauriPage, 'partial')
      expect(found).toBe(true)

      await dispatchMenuCommand(tauriPage, 'file.copy')
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
      await clickTransferStart(tauriPage)

      // Wait for a progress event proving we're mid-copy. `pollUntil` is fine
      // here: the underlying signal is event-driven (events arrive in the
      // buffer as the backend fires them), the poll is just how the Node-side
      // test sees the buffer. Each iteration reads the in-memory array, no
      // FS roundtrip. 10 s budget covers ~50 progress events at the backend's
      // 200 ms throttle (10 files × 200 ms = 2 s worst case here).
      const midCopySeen = await pollUntil(
        tauriPage,
        async () =>
          tauriPage.evaluate<boolean>(
            `(window.__cancelCopyTestEvents ?? []).some(p => p.phase === 'copying' && p.filesDone >= 1 && p.filesDone < p.filesTotal)`,
          ),
        10000,
        25,
      )
      expect(midCopySeen).toBe(true)

      // Click Rollback on the progress dialog.
      const clicked = await tauriPage.evaluate<boolean>(`(function(){
        var btns = document.querySelectorAll('[data-dialog-id="transfer-progress"] button');
        for (var i=0; i<btns.length; i++) {
          if (btns[i].textContent.trim().toLowerCase() === 'rollback') {
            btns[i].click();
            return true;
          }
        }
        return false;
      })()`)
      expect(clicked).toBe(true)

      // Wait for rollback to finish and dialogs to close.
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)

      // Rollback must remove the partial files and the directory we created
      // for them. Either right/partial/ doesn't exist, or it's empty.
      const rightPartial = path.join(fixtureRoot, 'right', 'partial')
      const exists = fs.existsSync(rightPartial)
      if (exists) {
        const remaining = fs.readdirSync(rightPartial)
        expect(remaining.length).toBe(0)
      }
    } finally {
      // Unlisten and clear test state, in that order so a partial failure can
      // still tear down cleanly.
      await tauriPage.evaluate(`(async function() {
        const id = window.__cancelCopyTestEventId;
        if (id !== undefined) {
          await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: 'write-progress', eventId: id });
        }
        delete window.__cancelCopyTestEvents;
        delete window.__cancelCopyTestEventId;
      })()`)
      await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
    }
  })
})

test.describe('Edge cases', () => {
  test('Sequential copy triggers conflict on second attempt', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    recreateFixtures(fixtureRoot)
    await ensureAppReady(tauriPage)

    // First copy: file-a.txt from left to right (no conflict)
    await moveCursorToFile(tauriPage, 'file-a.txt')
    await dispatchMenuCommand(tauriPage, 'file.copy')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    expect(fileExists(fixtureRoot, 'right/file-a.txt')).toBe(true)

    // Second copy: same file again (now there IS a conflict)
    await moveCursorToFile(tauriPage, 'file-a.txt')
    await dispatchMenuCommand(tauriPage, 'file.copy')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // File still exists with source content (overwritten with same content)
    expect(fileExists(fixtureRoot, 'right/file-a.txt')).toBe(true)
  })

  test('Copy with Overwrite All handles single-file conflict', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    recreateFixtures(fixtureRoot)

    // Manually create a conflicting file in right/
    writeFile(fixtureRoot, 'right/file-a.txt', 'original-dest')
    await ensureAppReady(tauriPage)

    await moveCursorToFile(tauriPage, 'file-a.txt')
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Source content overwrote dest (standard fixtures use 1024 'A' chars)
    const content = readFile(fixtureRoot, 'right/file-a.txt')
    expect(content).not.toBe('original-dest')
    expect(content.length).toBe(1024)
  })
})

test.describe('Symlink conflicts', () => {
  test('Copy with Overwrite All replaces regular file with symlink', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createSymlinkFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['link-target.txt'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Non-conflicting file copied
    expect(readFile(fixtureRoot, 'right/link-target.txt')).toBe('link-target-content')

    // my-link was overwritten: now a symlink pointing to link-target.txt
    const myLinkPath = path.join(fixtureRoot, 'right', 'my-link')
    const stat = fs.lstatSync(myLinkPath)
    expect(stat.isSymbolicLink()).toBe(true)

    // The symlink target is relative, and link-target.txt exists in right/,
    // so reading through the symlink should work
    const target = fs.readlinkSync(myLinkPath)
    expect(target).toBe('link-target.txt')
    expect(fs.readFileSync(myLinkPath, 'utf-8')).toBe('link-target-content')
  })

  test('Copy with Skip All preserves dest file, copies non-conflicting', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createSymlinkFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['link-target.txt'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'skip')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // Non-conflicting file still copied
    expect(readFile(fixtureRoot, 'right/link-target.txt')).toBe('link-target-content')

    // Conflicting my-link kept its original content (regular file, not symlink)
    const myLinkPath = path.join(fixtureRoot, 'right', 'my-link')
    const stat = fs.lstatSync(myLinkPath)
    expect(stat.isSymbolicLink()).toBe(false)
    expect(fs.readFileSync(myLinkPath, 'utf-8')).toBe('dest-my-link')
  })
})

test.describe('Type mismatch conflicts', () => {
  test('Copy with Overwrite All handles file-over-directory', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createTypeMismatchFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['reports.txt'] })

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(tauriPage)
    await selectConflictPolicy(tauriPage, 'overwrite')
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // reports.txt: source file overwrites dest directory
    const reportsPath = path.join(fixtureRoot, 'right', 'reports.txt')
    const reportsStat = fs.lstatSync(reportsPath)
    expect(reportsStat.isFile()).toBe(true)
    expect(fs.readFileSync(reportsPath, 'utf-8')).toBe('source-reports')

    // config/: source directory replaces dest file
    expect(readFile(fixtureRoot, 'right/config/settings.json')).toBe('source-settings')
  })

  test('Copy with Overwrite All handles directory-over-file', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    await ensureAppReady(tauriPage)

    // Use a simpler fixture: only the dir→file case
    clearFixtureDirs(fixtureRoot)
    writeFile(fixtureRoot, 'left/config/settings.json', 'source-settings')
    writeFile(fixtureRoot, 'right/config', 'dest-config')

    // Re-navigate panes to pick up new fixture
    const leftPath = fixtureRoot + '/left'
    const rightPath = fixtureRoot + '/right'
    await tauriPage.evaluate(`(function() {
      var invoke = window.__TAURI_INTERNALS__.invoke;
      invoke('plugin:event|emit', {
        event: 'mcp-nav-to-path',
        payload: { pane: 'left', path: ${JSON.stringify(leftPath)} }
      });
      invoke('plugin:event|emit', {
        event: 'mcp-nav-to-path',
        payload: { pane: 'right', path: ${JSON.stringify(rightPath)} }
      });
    })()`)

    const ready = await pollUntil(
      tauriPage,
      async () => {
        return tauriPage.evaluate<boolean>(`(function() {
          var pane = document.querySelectorAll('.file-pane')[0];
          if (!pane) return false;
          return !!pane.querySelector('[data-filename="config"]');
        })()`)
      },
      3000,
    )
    expect(ready).toBe(true)

    await tauriPage.evaluate(`(function() {
      var entry = document.querySelectorAll('.file-pane')[0]?.querySelector('.file-entry');
      if (entry) entry.click();
      var explorer = document.querySelector('.dual-pane-explorer');
      if (explorer) explorer.focus();
    })()`)
    await tauriPage.waitForSelector('.file-pane .file-entry.is-under-cursor', 3000)

    await selectAll(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.copy')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    // This might or might not show conflict policy depending on how the dry-run
    // detects the type mismatch. Wait long enough for the dry-run to settle, then
    // peek: we explicitly want to accept BOTH outcomes (visible vs. not visible),
    // so there's no observable signal to poll for: polling for `.conflict-policy`
    // to appear would mask the legitimate "no conflict UI" case.
    // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- dry-run-completion peek; intentionally tolerates both "conflict UI shown" and "no conflict UI" outcomes, so polling for a specific selector would mask the second case
    await sleep(1000)
    const hasConflict = await tauriPage.isVisible(`${TRANSFER_DIALOG} .conflict-policy`)
    if (hasConflict) {
      await selectConflictPolicy(tauriPage, 'overwrite')
    }
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    // config/ directory replaced the file
    const configPath = path.join(fixtureRoot, 'right', 'config')
    const configStat = fs.lstatSync(configPath)
    expect(configStat.isDirectory()).toBe(true)
    expect(readFile(fixtureRoot, 'right/config/settings.json')).toBe('source-settings')
  })
})
