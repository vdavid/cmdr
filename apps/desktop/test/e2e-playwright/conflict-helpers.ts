/**
 * Shared helpers for conflict resolution E2E tests.
 *
 * Contains fixture creation functions (disk-level) and UI action helpers
 * used by conflict-copy.spec.ts, conflict-move.spec.ts, and
 * conflict-edge-cases.spec.ts.
 */

import fs from 'fs'
import path from 'path'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { expect } from './fixtures.js'
import {
  pollUntil,
  sleep,
  TRANSFER_DIALOG,
} from './helpers.js'

/** Union type for tauriPage — works in both Tauri and browser mode. */
type PageLike = TauriPage | BrowserPageAdapter

// ── Fixture helpers ──────────────────────────────────────────────────────────

/**
 * Clears left/ and right/ directories (preserving left/bulk/) and creates
 * Layout A: simple nested conflicts at 3 depth levels.
 */
export function createConflictFixturesA(fixtureRoot: string): void {
  clearFixtureDirs(fixtureRoot)

  // left/
  writeFile(fixtureRoot, 'left/readme.txt', 'source-readme')
  writeFile(fixtureRoot, 'left/only-in-source.txt', 'only-in-source')
  writeFile(fixtureRoot, 'left/docs/guide.txt', 'source-guide')
  writeFile(fixtureRoot, 'left/docs/only-in-source-deep.txt', 'only-in-source-deep')
  writeFile(fixtureRoot, 'left/docs/nested/config.txt', 'source-config')

  // right/
  writeFile(fixtureRoot, 'right/readme.txt', 'dest-readme')
  writeFile(fixtureRoot, 'right/only-in-dest.txt', 'only-in-dest')
  writeFile(fixtureRoot, 'right/docs/guide.txt', 'dest-guide')
  writeFile(fixtureRoot, 'right/docs/only-in-dest-deep.txt', 'only-in-dest-deep')
  writeFile(fixtureRoot, 'right/docs/nested/config.txt', 'dest-config')
}

/**
 * Clears left/ and right/ directories (preserving left/bulk/) and creates
 * Layout B: multi-item merge with partial directory overlaps.
 */
export function createConflictFixturesB(fixtureRoot: string): void {
  clearFixtureDirs(fixtureRoot)

  // left/
  writeFile(fixtureRoot, 'left/alpha/info.txt', 'alpha-info')
  writeFile(fixtureRoot, 'left/bravo/payload.txt', 'bravo-payload')
  writeFile(fixtureRoot, 'left/bravo/foxtrot/golf.txt', 'source-golf')
  writeFile(fixtureRoot, 'left/charlie/data.txt', 'charlie-data')
  writeFile(fixtureRoot, 'left/delta.txt', 'delta-content')

  // right/
  writeFile(fixtureRoot, 'right/bravo/echo.txt', 'bravo-echo')
  writeFile(fixtureRoot, 'right/bravo/foxtrot/golf.txt', 'dest-golf')
  writeFile(fixtureRoot, 'right/bravo/foxtrot/hotel.txt', 'bravo-hotel')
}

/**
 * Clears left/ and right/ directories (preserving left/bulk/) and creates
 * Layout C: symlink in source, regular file with same name in dest.
 */
export function createSymlinkFixture(fixtureRoot: string): void {
  clearFixtureDirs(fixtureRoot)

  // left/
  writeFile(fixtureRoot, 'left/link-target.txt', 'link-target-content')
  // Create symlink: left/my-link → link-target.txt (relative target)
  const symlinkPath = path.join(fixtureRoot, 'left', 'my-link')
  fs.symlinkSync('link-target.txt', symlinkPath)

  // right/
  writeFile(fixtureRoot, 'right/my-link', 'dest-my-link')
}

/**
 * Clears left/ and right/ directories (preserving left/bulk/) and creates
 * Layout D: type mismatches between source and dest.
 *
 * Case 1 (file→dir): left/reports.txt is a file, right/reports.txt is a directory
 * Case 2 (dir→file): left/config/ is a directory, right/config is a file
 */
export function createTypeMismatchFixture(fixtureRoot: string): void {
  clearFixtureDirs(fixtureRoot)

  // Case 1: file in source, directory in dest
  writeFile(fixtureRoot, 'left/reports.txt', 'source-reports')
  writeFile(fixtureRoot, 'right/reports.txt/data.csv', 'dest-data')

  // Case 2: directory in source, file in dest
  writeFile(fixtureRoot, 'left/config/settings.json', 'source-settings')
  writeFile(fixtureRoot, 'right/config', 'dest-config')
}

/** Removes all contents of left/ (except bulk/) and right/. */
export function clearFixtureDirs(fixtureRoot: string): void {
  const leftDir = path.join(fixtureRoot, 'left')
  if (fs.existsSync(leftDir)) {
    for (const entry of fs.readdirSync(leftDir)) {
      if (entry === 'bulk') continue
      fs.rmSync(path.join(leftDir, entry), { recursive: true, force: true })
    }
  }

  const rightDir = path.join(fixtureRoot, 'right')
  if (fs.existsSync(rightDir)) {
    for (const entry of fs.readdirSync(rightDir)) {
      fs.rmSync(path.join(rightDir, entry), { recursive: true, force: true })
    }
  }

  fs.mkdirSync(leftDir, { recursive: true })
  fs.mkdirSync(rightDir, { recursive: true })
}

/** Creates a file with the given content, creating parent dirs as needed. */
export function writeFile(fixtureRoot: string, relPath: string, content: string): void {
  const fullPath = path.join(fixtureRoot, relPath)
  fs.mkdirSync(path.dirname(fullPath), { recursive: true })
  fs.writeFileSync(fullPath, content)
}

/** Reads a file's content as UTF-8 string. */
export function readFile(fixtureRoot: string, relPath: string): string {
  return fs.readFileSync(path.join(fixtureRoot, relPath), 'utf-8')
}

/** Checks if a file exists. */
export function fileExists(fixtureRoot: string, relPath: string): boolean {
  return fs.existsSync(path.join(fixtureRoot, relPath))
}

// ── UI action helpers ────────────────────────────────────────────────────────

/** Selects all items in the focused pane via Cmd+A / Ctrl+A. */
export async function selectAll(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
    var el=document.activeElement||document.body;
    el.dispatchEvent(new KeyboardEvent('keydown',{key:'a',bubbles:true,metaKey:${process.platform === 'darwin'},ctrlKey:${process.platform !== 'darwin'}}));
  })()`)
  await sleep(200)
}

/** Waits for the dry-run scan to detect conflicts and show the policy radio buttons. */
export async function waitForConflictPolicy(tauriPage: PageLike): Promise<void> {
  const found = await pollUntil(
    tauriPage,
    async () => tauriPage.isVisible(`${TRANSFER_DIALOG} .conflict-policy`),
    15000,
  )
  expect(found).toBe(true)
}

/** Selects a conflict resolution policy radio button (skip, overwrite, or stop). */
export async function selectConflictPolicy(tauriPage: PageLike, policy: 'skip' | 'overwrite' | 'stop'): Promise<void> {
  await tauriPage.click(`${TRANSFER_DIALOG} .conflict-policy input[value="${policy}"]`)
  await sleep(100)
}

/** Clicks the primary action button in the transfer dialog. */
export async function clickTransferStart(tauriPage: PageLike): Promise<void> {
  await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
  await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
}

/** Waits for all modal dialogs to close after an operation completes. */
export async function waitForDialogsToClose(tauriPage: PageLike): Promise<void> {
  const closed = await pollUntil(
    tauriPage,
    async () => !(await tauriPage.isVisible('.modal-overlay')),
    15000,
  )
  expect(closed).toBe(true)
}
