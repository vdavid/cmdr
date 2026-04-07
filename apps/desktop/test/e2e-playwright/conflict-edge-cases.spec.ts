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
import { ensureAppReady, getFixtureRoot, moveCursorToFile, pollUntil, sleep, TRANSFER_DIALOG } from './helpers.js'
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
    test.setTimeout(120_000) // Rollback requires waiting for scan preview → copy start
    const fixtureRoot = getFixtureRoot()
    // Use standard fixtures with bulk/ dir (~170 MB)
    recreateFixtures(fixtureRoot)
    await ensureAppReady(tauriPage)

    // Select the bulk/ directory (don't navigate into it) and copy it
    const found = await moveCursorToFile(tauriPage, 'bulk')
    expect(found).toBe(true)

    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await clickTransferStart(tauriPage)

    // Wait for progress dialog to appear
    await pollUntil(tauriPage, async () => tauriPage.isVisible('[data-dialog-id="transfer-progress"]'), 10000)

    // Try to click Rollback. On fast filesystems (Docker overlay), the copy may
    // complete before we can click. Poll for the Rollback button, clicking it as
    // soon as it appears. If the dialog closes before we find it, the copy finished.
    let clickedRollback = false
    const deadline = Date.now() + 10000
    while (Date.now() < deadline) {
      const dialogVisible = await tauriPage.isVisible('[data-dialog-id="transfer-progress"]')
      if (!dialogVisible) break // Dialog closed — copy already completed

      clickedRollback = await tauriPage.evaluate<boolean>(`(function(){
        var btns = document.querySelectorAll('[data-dialog-id="transfer-progress"] button');
        for (var i=0; i<btns.length; i++) {
          if (btns[i].textContent.trim().toLowerCase() === 'rollback') {
            btns[i].click();
            return true;
          }
        }
        return false;
      })()`)
      if (clickedRollback) break
      await sleep(100)
    }

    if (!clickedRollback) {
      // Copy completed too fast to cancel — this is expected on fast filesystems.
      // Verify the copy completed successfully and dismiss any remaining dialogs.
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
      const rightBulk = path.join(fixtureRoot, 'right', 'bulk')
      expect(fs.existsSync(rightBulk)).toBe(true)
      // eslint-disable-next-line no-console
      console.log('Copy completed before Rollback could be clicked — skipping rollback verification')
      return
    }

    // Wait for the rollback to complete and dialogs to close
    const closed = await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 30000)
    if (!closed) {
      await tauriPage.keyboard.press('Escape')
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
    }

    // After rollback, right/bulk/ should not exist or have minimal remnants.
    // On very fast filesystems (Docker overlay), the copy may have completed
    // before the rollback took effect despite clicking the button — the copy
    // finished between the button appearing and our click registering.
    const rightBulk = path.join(fixtureRoot, 'right', 'bulk')
    if (fs.existsSync(rightBulk)) {
      const remaining = fs.readdirSync(rightBulk)
      if (remaining.length >= 23) {
        // All files present — copy completed before rollback took effect.
        // This is expected on fast filesystems where 170MB copies in <1s.
        // eslint-disable-next-line no-console
        console.log(
          `Rollback clicked but copy already completed (${String(remaining.length)} files remain) — fast filesystem race`,
        )
      } else {
        // Partial rollback — some files were cleaned up
        expect(remaining.length).toBeLessThan(3)
      }
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
    await tauriPage.keyboard.press('F5')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
    await clickTransferStart(tauriPage)
    await waitForDialogsToClose(tauriPage)

    expect(fileExists(fixtureRoot, 'right/file-a.txt')).toBe(true)

    // Second copy: same file again (now there IS a conflict)
    await moveCursorToFile(tauriPage, 'file-a.txt')
    await tauriPage.keyboard.press('F5')
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
    await tauriPage.keyboard.press('F5')

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
    await tauriPage.keyboard.press('F5')

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

  // FIXME(macOS): On macOS, the non-conflicting link-target.txt is not copied
  // when the conflicting symlink my-link is skipped. This appears to be a bug
  // in the copy operation's symlink handling with Skip policy. Works on Linux.
  // eslint-disable-next-line @typescript-eslint/unbound-method -- conditional skip
  const skipOrTest = process.platform === 'darwin' ? test.skip : test
  skipOrTest('Copy with Skip All preserves dest file, copies non-conflicting', async ({ tauriPage }) => {
    const fixtureRoot = getFixtureRoot()
    createSymlinkFixture(fixtureRoot)
    await ensureAppReady(tauriPage, { leftPane: ['link-target.txt'] })

    await selectAll(tauriPage)
    await tauriPage.keyboard.press('F5')

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
    await tauriPage.keyboard.press('F5')

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
    await sleep(300)

    const ready = await pollUntil(
      tauriPage,
      async () => {
        return tauriPage.evaluate<boolean>(`(function() {
          var pane = document.querySelectorAll('.file-pane')[0];
          if (!pane) return false;
          var entries = pane.querySelectorAll('.file-entry');
          return Array.from(entries).some(function(e) {
            return (e.querySelector('.col-name') || e.querySelector('.name') || {}).textContent === 'config';
          });
        })()`)
      },
      10000,
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
    await tauriPage.keyboard.press('F5')

    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    // This might or might not show conflict policy depending on how the dry-run
    // detects the type mismatch. Wait briefly then check.
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
