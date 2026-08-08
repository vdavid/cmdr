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
import { pollUntil, TRANSFER_DIALOG } from './helpers.js'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'

/** Union type for tauriPage (works in both Tauri and browser mode). */
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
  // Defensive remove: on macOS, the app's file watcher can race with fixture
  // cleanup and recreate the path between clearFixtureDirs and symlinkSync.
  const symlinkPath = path.join(fixtureRoot, 'left', 'my-link')
  fs.rmSync(symlinkPath, { force: true })
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

/**
 * Removes a single fixture entry, including the dangling-symlink edge case.
 *
 * `fs.rmSync(p, { recursive: true, force: true })` silently no-ops on a dangling
 * symlink (target missing), because `force: true` swallows the underlying
 * ENOENT. Iterating siblings can produce exactly that state: removing
 * `link-target.txt` BEFORE `my-link` (a symlink to it) leaves `my-link`
 * dangling, then `rmSync` on `my-link` does nothing. We `lstat` first and call
 * `unlinkSync` directly on symlinks so they always get removed.
 */
function removeFixtureEntry(entry: string): void {
  let stat: fs.Stats | undefined
  try {
    stat = fs.lstatSync(entry)
  } catch {
    return
  }
  if (stat.isSymbolicLink()) {
    fs.unlinkSync(entry)
    return
  }
  fs.rmSync(entry, { recursive: true, force: true })
}

/** Removes all contents of left/ (except bulk/) and right/. */
export function clearFixtureDirs(fixtureRoot: string): void {
  const leftDir = path.join(fixtureRoot, 'left')
  if (fs.existsSync(leftDir)) {
    for (const entry of fs.readdirSync(leftDir)) {
      if (entry === 'bulk') continue
      removeFixtureEntry(path.join(leftDir, entry))
    }
  }

  const rightDir = path.join(fixtureRoot, 'right')
  if (fs.existsSync(rightDir)) {
    for (const entry of fs.readdirSync(rightDir)) {
      removeFixtureEntry(path.join(rightDir, entry))
    }
  }

  fs.mkdirSync(leftDir, { recursive: true })
  fs.mkdirSync(rightDir, { recursive: true })
}

/**
 * Creates a file with the given content, creating parent dirs as needed.
 *
 * Defensively unlinks the target first if it's a stale symlink. Without this,
 * `fs.writeFileSync` follows the link and writes to the target path instead —
 * which can resurrect a previously-deleted sibling and break subsequent
 * conflict-scan expectations.
 */
export function writeFile(fixtureRoot: string, relPath: string, content: string): void {
  const fullPath = path.join(fixtureRoot, relPath)
  fs.mkdirSync(path.dirname(fullPath), { recursive: true })
  try {
    const stat = fs.lstatSync(fullPath)
    if (stat.isSymbolicLink()) {
      fs.unlinkSync(fullPath)
    }
  } catch {
    // Path doesn't exist yet; writeFileSync will create it.
  }
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

/**
 * Reads the row index of every requested name from the focused pane in ONE
 * evaluate, so the indices are mutually consistent: a listing reload between
 * two per-name reads would otherwise hand back indices from two different
 * listings, and `select` would add the wrong rows. While the pane is missing any
 * of them it reports which ones instead of indices.
 */
async function findIndicesInFocusedPane(
  tauriPage: PageLike,
  names: string[],
): Promise<{ indices: number[] } | { missing: string[] }> {
  const result = await tauriPage.evaluate<{ indices: number[] } | { missing: string[] }>(`(function() {
        var wanted = ${JSON.stringify(names)};
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return { missing: wanted };
        var entries = pane.querySelectorAll('.file-entry');
        var byName = {};
        for (var i = 0; i < entries.length; i++) {
            byName[entries[i].getAttribute('data-filename')] = i;
        }
        var indices = [], missing = [];
        for (var j = 0; j < wanted.length; j++) {
            if (byName[wanted[j]] === undefined) missing.push(wanted[j]);
            else indices.push(byName[wanted[j]]);
        }
        return missing.length > 0 ? { missing: missing } : { indices: indices };
    })()`)
  return result
}

/** Reads the names of every selected row in the focused pane. */
async function selectedNamesInFocusedPane(tauriPage: PageLike): Promise<string[]> {
  return tauriPage.evaluate<string[]>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused');
        if (!pane) return [];
        var rows = pane.querySelectorAll('.file-entry.is-selected');
        var names = [];
        for (var i = 0; i < rows.length; i++) names.push(rows[i].getAttribute('data-filename'));
        return names.sort();
    })()`)
}

/**
 * Selects exactly the named top-level items in the focused (left) pane via the
 * MCP `select` tool (`mode: add` per item). Use this instead of `selectAll`
 * for conflict matrix tests: `selectAll` also grabs the shared `bulk/` .dat
 * files and the standard fixture text files, so the op copies dozens of
 * unrelated files and the "Copy complete: copied N files" toast count balloons.
 * Selecting only the fixture's own items keeps the op scoped to the clash(es)
 * under test.
 *
 * Structurally race-free in three steps, because every conflict spec's
 * `recreateFixtures` (beforeEach) deletes then recreates `left/` on disk and the
 * watcher's debounced remove/create diffs can drain just AFTER `ensureAppReady`'s
 * files-present poll, briefly emptying the pane (the same window that made
 * `moveCursorToFile` poll instead of read once):
 *
 * 1. Wait for the pane to list ALL requested names, reading their indices in one
 *    snapshot so a reload can't interleave between two per-name reads.
 * 2. Issue the `select` calls.
 * 3. Assert the resulting selection is EXACTLY the requested set, retrying the
 *    whole cycle if a reload landed mid-selection. Without step 3 a listing that
 *    reloaded between the read and the select would silently copy the wrong
 *    files, and the spec would fail somewhere far downstream (or pass wrongly).
 *
 * A genuinely absent file still fails after the deadline, naming the file.
 */
export async function selectItemsByName(tauriPage: PageLike, names: string[]): Promise<void> {
  await ensureMcpClient(tauriPage)
  const wanted = [...names].sort()
  const matchesWanted = (selected: string[]): boolean =>
    selected.length === wanted.length && selected.every((name, i) => name === wanted[i])

  // Three attempts, each waiting up to 5 s for the pane to list every name.
  // 5 s is failure headroom, not an expected wait: a reload settles in well under
  // a second even on the Docker lane, so a green run returns on the first attempt.
  // The retries exist only for a reload landing between the index read and the
  // select; two of them is already one more than has ever been observed to be
  // needed, and a genuinely wrong selection still fails.
  let lastFailure = 'no attempt completed'
  for (let attempt = 0; attempt < 3; attempt++) {
    const listed = await pollUntil(
      tauriPage,
      async () => !('missing' in (await findIndicesInFocusedPane(tauriPage, names))),
      5000,
    )
    // Re-read rather than capture inside the poll: a `let` assigned from a
    // callback defeats control-flow narrowing, and the re-read costs one extra
    // `evaluate` on the happy path. If a reload slipped in between, this attempt
    // reports it and the next one starts over.
    const found = await findIndicesInFocusedPane(tauriPage, names)
    if (!listed || 'missing' in found) {
      const missing = 'missing' in found ? found.missing.map((name) => `'${name}'`).join(', ') : '(unknown)'
      lastFailure = `pane never listed ${missing}`
      continue
    }

    await mcpCall('select', { pane: 'left', count: 0 })
    for (const index of found.indices) {
      await mcpCall('select', { pane: 'left', start: index, count: 1, mode: 'add' })
    }

    // The selection is what the op under test acts on, so confirm it landed
    // exactly rather than assuming the `select` calls hit the rows we read.
    const settled = await pollUntil(
      tauriPage,
      async () => matchesWanted(await selectedNamesInFocusedPane(tauriPage)),
      2000,
    )
    if (settled) return
    lastFailure = `selection landed on [${(await selectedNamesInFocusedPane(tauriPage)).join(', ')}] instead of [${wanted.join(', ')}]`
  }
  throw new Error(`selectItemsByName: ${lastFailure}`)
}

/** Selects all items in the focused pane via Cmd+A / Ctrl+A. */
export async function selectAll(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
    var el=document.activeElement||document.body;
    el.dispatchEvent(new KeyboardEvent('keydown',{key:'a',bubbles:true,metaKey:${String(process.platform === 'darwin')},ctrlKey:${String(process.platform !== 'darwin')}}));
  })()`)
  // Wait for the selection to actually register rather than a fixed settle:
  // selected rows carry `.is-selected`, so polling for one is the real signal.
  await expect
    .poll(async () => tauriPage.evaluate<number>(`document.querySelectorAll('.is-selected').length`), { timeout: 2000 })
    .toBeGreaterThan(0)
}

/** Waits for the dry-run scan to detect conflicts and show the policy radio buttons. */
export async function waitForConflictPolicy(tauriPage: PageLike): Promise<void> {
  // 6 s: the dry-run finishes in well under 1 s now that the conflict specs
  // select only their own small fixtures (via `selectItemsByName`) rather than
  // `selectAll` sweeping in the ~170 MB `bulk/` tree. 6 s is a modest margin for
  // Docker-lane jitter that keeps fast failure signal; it briefly ran at 12 s to
  // absorb the bulk/ scan before the scoping landed.
  const found = await pollUntil(tauriPage, async () => tauriPage.isVisible(`${TRANSFER_DIALOG} .conflict-policy`), 6000)
  if (!found) throw new Error('waitForConflictPolicy: .conflict-policy radio buttons did not appear within 6s')
}

/** Selects a conflict resolution policy radio button. */
export async function selectConflictPolicy(
  tauriPage: PageLike,
  policy: 'skip' | 'overwrite' | 'overwrite_smaller' | 'overwrite_older' | 'stop',
): Promise<void> {
  const radio = `${TRANSFER_DIALOG} .conflict-policy input[value="${policy}"]`
  await tauriPage.click(radio)
  // Confirm the radio registered as checked rather than a fixed settle.
  const radioSel = JSON.stringify(radio)
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(
          `!!document.querySelector(${radioSel}) && document.querySelector(${radioSel}).checked`,
        ),
      {
        timeout: 2000,
      },
    )
    .toBeTruthy()
}

/** Clicks the primary action button in the transfer dialog. */
export async function clickTransferStart(tauriPage: PageLike): Promise<void> {
  await tauriPage.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 3000)
  await tauriPage.click(`${TRANSFER_DIALOG} .btn-primary`)
}

/** Waits for all modal dialogs to close after an operation completes. */
export async function waitForDialogsToClose(tauriPage: PageLike, timeout = 12000): Promise<void> {
  // 12 s default: the op closes the dialog in <1 s normally (the conflict specs
  // now select only their own small fixtures, not `selectAll`'s ~170 MB `bulk/`
  // tree), but the dialog UNMOUNT has rarely lagged several seconds under
  // Docker-lane load even after the op finished — seen on the dir-over-file
  // atomic-aside overwrite (conflict-edge-cases:296) where the "Copy complete"
  // toast had already fired. 12 s absorbs that transient; a green run resolves
  // the instant the overlay is gone, so this costs nothing on the happy path and
  // only lengthens a genuine failure. (The sibling `waitForConflictPolicy` scan
  // wait stays at 6 s — no scan-side stall has surfaced.) Callers can pass tighter.
  const closed = await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), timeout)
  if (!closed) {
    throw new Error(`waitForDialogsToClose: .modal-overlay still visible after ${String(timeout)}ms`)
  }
}

/**
 * Clicks a button by its trimmed text within `containerSelector`. Retries until
 * either the click lands or the timeout expires. Guards against Svelte rendering
 * the container without its inner buttons yet; a plain
 * `querySelectorAll(...).click()` would silently no-op on an empty NodeList and
 * the test would then sit waiting for the next UI state that never comes.
 *
 * Used by the conflict flows where the buttons appear inside `.conflict-section`
 * (`.conflict-buttons-row`, `.conflict-cancel`) but may render a frame after
 * their container becomes visible.
 */
export async function clickConflictButton(
  tauriPage: PageLike,
  containerSelector: string,
  buttonText: string,
  timeout = 2000,
): Promise<void> {
  const sel = JSON.stringify(containerSelector)
  const txt = JSON.stringify(buttonText)
  const clicked = await pollUntil(
    tauriPage,
    async () =>
      tauriPage.evaluate<boolean>(`(function(){
        var btns = document.querySelectorAll(${sel});
        for (var i = 0; i < btns.length; i++) {
          if ((btns[i].textContent || '').trim() === ${txt}) {
            btns[i].click();
            return true;
          }
        }
        return false;
      })()`),
    timeout,
  )
  expect(
    clicked,
    `clickConflictButton: no "${buttonText}" button under "${containerSelector}" within ${String(timeout)}ms`,
  ).toBe(true)
}

// ── Matrix helpers (state-machine spec) ──────────────────────────────────────

/** Snapshot of the currently-rendered per-file conflict dialog. */
export interface ConflictSnapshot {
  filename: string
  isFileOverFolder: boolean
}

/**
 * Reads the currently-displayed conflict's filename + whether it's the
 * file→folder (red warning) variant. Returns null when no conflict is shown.
 */
export async function readCurrentConflict(tauriPage: PageLike): Promise<ConflictSnapshot | null> {
  return tauriPage.evaluate<ConflictSnapshot | null>(`(function(){
    var section = document.querySelector('.conflict-section');
    if (!section) return null;
    var nameEl = section.querySelector('.conflict-filename');
    if (!nameEl) return null;
    var name = (nameEl.textContent || '').trim();
    if (!name) return null;
    var hasWarning = !!section.querySelector('.conflict-warning');
    return { filename: name, isFileOverFolder: hasWarning };
  })()`)
}

/** Waits for a conflict dialog to appear and returns its snapshot. */
export async function waitForConflict(tauriPage: PageLike, timeout = 5000): Promise<ConflictSnapshot> {
  const found = await pollUntil(tauriPage, async () => (await readCurrentConflict(tauriPage)) !== null, timeout)
  const snapshot = found ? await readCurrentConflict(tauriPage) : null
  if (snapshot === null) {
    throw new Error(`waitForConflict: no .conflict-section appeared within ${String(timeout)}ms`)
  }
  return snapshot
}

/**
 * After resolving a conflict, waits for either the next conflict to appear
 * (different filename) OR the whole transfer-progress dialog to close
 * (operation done). Returns the next snapshot, or null when the operation
 * finished with no further conflict.
 *
 * Why not "conflict section gone == done": between resolving one conflict and
 * the next `write-conflict` event arriving, `conflictEvent` is briefly null, so
 * `.conflict-section` unmounts for a frame or two while the progress dialog
 * stays open. A naive "null means done" check races that gap and reports a
 * false "no second prompt." We instead treat null as "done" ONLY when the
 * progress dialog itself is gone; a null with the dialog still up means "next
 * conflict pending," so we keep polling.
 */
export async function waitForNextConflictOrDone(
  tauriPage: PageLike,
  previous: ConflictSnapshot,
  timeout = 5000,
): Promise<ConflictSnapshot | null> {
  const progressDialogSel = '[data-dialog-id="transfer-progress"]'
  const settled = await pollUntil(
    tauriPage,
    async () => {
      const current = await readCurrentConflict(tauriPage)
      if (current !== null) {
        // A different conflict is showing → next prompt.
        return current.filename !== previous.filename
      }
      // No conflict section right now. Only "done" if the progress dialog
      // closed too; otherwise it's the transient gap between conflicts.
      return !(await tauriPage.isVisible(progressDialogSel))
    },
    timeout,
  )
  if (!settled) {
    throw new Error(`waitForNextConflictOrDone: dialog stayed on '${previous.filename}' for ${String(timeout)}ms`)
  }
  // Re-read after the poll: either a different conflict is up, or it's null
  // (op finished). The transient-gap case is excluded by the poll condition.
  const next = await readCurrentConflict(tauriPage)
  return next !== null && next.filename === previous.filename ? null : next
}

/**
 * Asserts no `.cmdr-temp-*` aside artifacts survived under `dirAbs`. The
 * folder↔file overwrite paths use `name.cmdr-temp-<uuid>` to move the
 * pre-existing entry aside; that aside MUST be cleaned up on success.
 */
export function expectNoTempArtifacts(dirAbs: string): void {
  if (!fs.existsSync(dirAbs)) return
  const leftover: string[] = []
  function walk(p: string): void {
    for (const name of fs.readdirSync(p)) {
      if (name.includes('.cmdr-temp-')) leftover.push(path.join(p, name))
      const full = path.join(p, name)
      let stat: fs.Stats
      try {
        stat = fs.lstatSync(full)
      } catch {
        continue
      }
      if (stat.isDirectory() && !stat.isSymbolicLink()) walk(full)
    }
  }
  walk(dirAbs)
  expect(leftover, `cmdr-temp artifacts found under ${dirAbs}: ${leftover.join(', ')}`).toEqual([])
}

/**
 * Builds an ordered-pair fixture batch: one file→file clash plus one
 * file→folder clash. The matrix tests inspect the rendered dialog (warning
 * block or not) to identify which bucket each conflict belongs to, so
 * filesystem-walk order doesn't break the assertions.
 *
 * Items are processed in the source pane's sort order (name ascending by
 * default), so the alphabetical order of `normalName` vs `pairName` decides
 * which clash prompts first. Callers that need a specific first-clash type
 * name them accordingly (for example `1-normal.txt` before `2-folder`).
 *
 * `pairName` is the name used for the file→folder pair (source = file,
 * dest = directory containing a single child file). `normalName` is the
 * file→file pair (source and dest both files, different content).
 */
export function createOrderedPairFixture(fixtureRoot: string, options: { normalName: string; pairName: string }): void {
  clearFixtureDirs(fixtureRoot)
  writeFile(fixtureRoot, `left/${options.normalName}`, `source-${options.normalName}`)
  writeFile(fixtureRoot, `right/${options.normalName}`, `dest-${options.normalName}`)
  writeFile(fixtureRoot, `left/${options.pairName}`, `source-${options.pairName}`)
  writeFile(fixtureRoot, `right/${options.pairName}/inside.txt`, `dest-inside-${options.pairName}`)
}

/**
 * Resolves the currently-shown per-file conflict by clicking a button whose
 * trimmed text exactly matches `buttonText` (for example `Skip`, `Skip all`,
 * `Overwrite`, `Overwrite folder with file`). Matches against every button in
 * the conflict dialog (action grid + the Skip/Rename/Overwrite rows), so it
 * disambiguates `Skip` from `Skip all` by exact text.
 */
export async function resolveConflict(tauriPage: PageLike, buttonText: string): Promise<void> {
  await clickConflictButton(tauriPage, '.conflict-buttons-row button', buttonText)
}

/**
 * Builds a folder→file fixture: source = a folder, destination = a file with
 * the same name. The folder contains one sentinel child the test can read
 * after an overwrite to confirm the source folder landed atomically.
 */
export function createFolderOverFileFixture(fixtureRoot: string, name: string): void {
  clearFixtureDirs(fixtureRoot)
  writeFile(fixtureRoot, `left/${name}/sentinel.txt`, `source-sentinel-${name}`)
  writeFile(fixtureRoot, `right/${name}`, `dest-${name}`)
}

/**
 * Builds a file→folder fixture: source = a file, destination = a folder
 * with one child. After an Overwrite, the destination becomes the file and
 * the folder's contents must be gone.
 */
export function createFileOverFolderFixture(fixtureRoot: string, name: string): void {
  clearFixtureDirs(fixtureRoot)
  writeFile(fixtureRoot, `left/${name}`, `source-${name}`)
  writeFile(fixtureRoot, `right/${name}/inside.txt`, `dest-inside-${name}`)
}

/**
 * Builds a file→file fixture (the baseline clash): source and dest are both
 * files with the same name but different content.
 */
export function createFileOverFileFixture(fixtureRoot: string, name: string): void {
  clearFixtureDirs(fixtureRoot)
  writeFile(fixtureRoot, `left/${name}`, `source-${name}`)
  writeFile(fixtureRoot, `right/${name}`, `dest-${name}`)
}

/**
 * Builds a folder→folder fixture (the other baseline clash): source and dest
 * are both folders with the same name, each holding a distinct child file plus
 * one shared-name child file that clashes inside.
 */
export function createFolderOverFolderFixture(fixtureRoot: string, name: string): void {
  clearFixtureDirs(fixtureRoot)
  writeFile(fixtureRoot, `left/${name}/shared.txt`, `source-shared-${name}`)
  writeFile(fixtureRoot, `left/${name}/only-source.txt`, `only-source-${name}`)
  writeFile(fixtureRoot, `right/${name}/shared.txt`, `dest-shared-${name}`)
  writeFile(fixtureRoot, `right/${name}/only-dest.txt`, `only-dest-${name}`)
}
