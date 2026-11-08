/**
 * E2E for rolling an operation back from the history dialog: the Roll back button,
 * the question it asks, and what actually happens to the files on disk.
 *
 * **What this suite is for, and what it leaves to other layers.** The engine's
 * semantics (per-kind inverses, the snapshot recheck, the never-overwrite restore,
 * the typed skip reasons) are pinned in Rust against in-memory volumes, in
 * `crates/cmdr-operations/src/rollback/planner_tests/tests.rs` — cheaply and exhaustively.
 * Repeating any of it here would buy nothing for minutes of wall clock. What only an
 * E2E can prove is the WIRING: that pressing a button in a real webview reaches the
 * real engine, that the engine's answer comes back typed and gets worded correctly,
 * and that real files on a real disk end up where the dialog promised. So every test
 * here starts from real files and ends on `fs`.
 *
 * The component tests (`src/lib/operation-log/OperationLogDialog.test.ts`) already
 * cover the same surfaces against a MOCKED backend, and the two aren't duplicates:
 * a mock can prove the dialog words a `move` row as a restore, but only a real run
 * proves that a real move IS journaled as a move, that the variant map still lines
 * up with the backend's `inverse_kind`, and that a wire value like
 * `alreadyRolledBack` is one the backend actually emits. Every one of those can
 * drift while every mock-backed test stays green.
 *
 * **Operations are staged over IPC; only the reversal is driven through the UI.**
 * `copy_between_volumes` and `move_files` are exactly what the F5 and F6 dialogs
 * dispatch for a local transfer (`transfer-dispatch.ts`: copy always goes through
 * `copyBetweenVolumes`, while a local→local move takes the `moveFiles` fast path and
 * NOT `moveBetweenVolumes`), so the journal rows these tests reverse are the rows a
 * user's own transfer writes. Going through the dialogs too would re-test what
 * `file-operations.spec.ts` already covers, at a modal round trip per test.
 *
 * **Timing comes from `set_test_rollback_throttle`, never from a sleep.** Reversing
 * six 1 KB files is over in single-digit milliseconds, so a test about a reversal IN
 * PROGRESS would otherwise be racing the engine. The throttle opens a known window
 * per item; the tests still wait on STATE (a file that's gone), never on a clock.
 * Hook docs: `docs/testing.md` § "E2E env-var hooks".
 *
 * **Left out on purpose:**
 * - The `volumeUnavailable` refusal. Staging it means a volume that vanishes between
 *   the transfer and the press, which costs an SMB fixture stack for a refusal that
 *   `rollback/tests.rs::entry_refuses_unknown_and_not_rollbackable_and_disconnected`
 *   already pins and `operation-log-labels.test.ts` already words.
 * - The ENGINE's progress, pause, and stop semantics, which are pinned in Rust
 *   against in-memory volumes (`crates/cmdr-operations/src/rollback/planner_tests/control_tests.rs`) with a
 *   real mid-file window. The three at the bottom here prove the same three reach a
 *   real webview, a real queue row, and real files on a real disk.
 *
 * Requires `--features playwright-e2e`.
 */

import { waitBudget } from './wait-budget.js'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  clickButtonByText,
  dismissAllToasts,
  ensureAppReady,
  escapeOverlayUntilGone,
  getFixtureRoot,
} from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import type { RollbackSelection } from '../../src/lib/ipc/bindings.js'

/** The history dialog, and the confirmation that stacks over it. */
const LOG_DIALOG = '[data-dialog-id="operation-log"]'
const CONFIRM_DIALOG = '[data-dialog-id="rollback-confirmation"]'

/**
 * Per-item pause for the tests that have to catch a reversal mid-flight. Long
 * enough that the poll which spots the first reversed item still has most of an
 * item's window left to act in, short enough that a six-item reversal can't outlive
 * the test budget if a poll goes the slow way.
 */
const ROLLBACK_THROTTLE_MS = 400

/**
 * The same, for the pause test, which spends a multiple of it proving a parked
 * reversal isn't moving. It's the only spec whose runtime is mostly deliberate
 * waiting, so it gets the shortest window a 25 ms poll can still land inside.
 */
const PAUSE_THROTTLE_MS = 120

/** Small files: these tests assert on presence, never on bytes moved. */
const FILE_BYTES = 1024

// ── Staging ──────────────────────────────────────────────────────────────────

/** Creates `left/<name>/` with `count` tiny files, Node-side on the real disk. */
function makeSourceDir(fixtureRoot: string, name: string, count: number): void {
  const dir = path.join(fixtureRoot, 'left', name)
  fs.mkdirSync(dir, { recursive: true })
  for (let i = 0; i < count; i++) {
    fs.writeFileSync(path.join(dir, `f-${String(i)}.txt`), 'x'.repeat(FILE_BYTES))
  }
}

/** `right/<name>/f-<i>.txt` — where a staged copy or move lands. */
function destFile(fixtureRoot: string, name: string, i: number): string {
  return path.join(fixtureRoot, 'right', name, `f-${String(i)}.txt`)
}

/** `left/<name>/f-<i>.txt` — where a moved file came from, and must return to. */
function sourceFile(fixtureRoot: string, name: string, i: number): string {
  return path.join(fixtureRoot, 'left', name, `f-${String(i)}.txt`)
}

/** How many of `count` destination files are gone. The state these tests wait on. */
function goneCount(fixtureRoot: string, name: string, count: number): number {
  let n = 0
  for (let i = 0; i < count; i++) if (!fs.existsSync(destFile(fixtureRoot, name, i))) n++
  return n
}

/**
 * Runs a local→local transfer through the production IPC and waits until the journal
 * has finished recording it, so the next read of the history sees a finished,
 * reversible row rather than a running one. Returns the new operation's id.
 */
async function stageTransfer(
  page: TauriPage,
  kind: 'copy' | 'move',
  fixtureRoot: string,
  name: string,
): Promise<string> {
  const src = JSON.stringify(path.join(fixtureRoot, 'left', name))
  const destDir = JSON.stringify(path.join(fixtureRoot, 'right'))
  // Two different commands, because that's what the transfer dialog does: a copy is
  // always `copyBetweenVolumes`, and a local→local move takes the `moveFiles` fast
  // path. Staging a local move through `move_between_volumes` would journal it via
  // the volume mover instead, and reverse a code path no user reaches here.
  const invocation =
    kind === 'copy'
      ? `window.__TAURI_INTERNALS__.invoke('copy_between_volumes', {
          sourceVolumeId: 'root', sourcePaths: [${src}], destVolumeId: 'root', destPath: ${destDir},
          config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] },
    initiator: 'user'
        })`
      : `window.__TAURI_INTERNALS__.invoke('move_files', {
          sources: [${src}], destination: ${destDir},
          config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] },
          initiator: 'user'
        })`
  // Which operations existed BEFORE, so the new one can be identified by
  // difference. ❌ Not "the newest row": `started_at` has second granularity, and a
  // previous test's reversal journals its own inverse operation in the same second,
  // so which of the two sorts first is genuinely undefined. That ambiguity failed
  // three tests here before this diff replaced it.
  const before = new Set((await recentEntries(page, 30)).map((r) => r.opId))
  await page.evaluate(`(async function() { await ${invocation}; })()`)

  // Readiness is "a NEW operation is done and reversible". Polling the journal
  // rather than a completion toast keeps this independent of notification copy, and
  // it's the same read the dialog will do a moment later.
  let opId = ''
  await expect
    .poll(
      async () => {
        const fresh = (await recentEntries(page, 30)).find((r) => !before.has(r.opId))
        if (!fresh || fresh.executionStatus !== 'done' || fresh.rollbackState !== 'rollbackable') return false
        opId = fresh.opId
        return true
      },
      { timeout: waitBudget(10000) },
    )
    .toBe(true)
  return opId
}

// ── Reading the journal ──────────────────────────────────────────────────────

interface LogRow {
  opId: string
  executionStatus: string
  rollbackState: string
  notRollbackableReason: string | null
}

/** The newest `limit` operations, straight from the journal (the same read the
 *  dialog does). Ground truth for a terminal state the dialog never re-reads. */
function recentEntries(page: TauriPage, limit: number): Promise<LogRow[]> {
  return page.evaluate<LogRow[]>(`(async function() {
    return await window.__TAURI_INTERNALS__.invoke('get_recent_operation_log_entries', { limit: ${String(limit)}, offset: 0 });
  })()`)
}

/** One operation's `rollbackState`, or `''` if it's fallen out of the newest page. */
async function rollbackStateOf(page: TauriPage, opId: string): Promise<string> {
  const rows = await recentEntries(page, 20)
  return rows.find((r) => r.opId === opId)?.rollbackState ?? ''
}

/**
 * Waits for `opId` to finish reversing, and returns which terminal state it reached
 * (`rolledBack` or `partiallyRolledBack`).
 *
 * Deliberately does NOT accept `rollbackable` as an answer even though the engine
 * can resolve back to it (a reversal that ran nothing at all). Every test here
 * expects SOMETHING to have come back, so a reversal that quietly reversed nothing
 * has to fail rather than read as settled.
 */
async function settledRollbackState(page: TauriPage, opId: string): Promise<string> {
  await expect
    .poll(async () => ['rolledBack', 'partiallyRolledBack'].includes(await rollbackStateOf(page, opId)), {
      timeout: waitBudget(20000),
    })
    .toBe(true)
  return rollbackStateOf(page, opId)
}

/**
 * Dispatches a rollback straight over IPC and reports what came back. The command
 * answers `Result<RollbackDispatch, RollbackRefusal>`, so a refusal REJECTS the
 * invoke with its typed payload: `refusal` is that payload's `kind` discriminant,
 * and `''` on success. Nothing here reads a message string.
 */
function dispatchRollback(
  page: TauriPage,
  opId: string,
  selection: RollbackSelection | null = null,
): Promise<{ ok: boolean; refusal: string }> {
  return page.evaluate<{ ok: boolean; refusal: string }>(`(async function() {
    try {
      await window.__TAURI_INTERNALS__.invoke('rollback_operation', {
        operationId: ${JSON.stringify(opId)}, initiator: 'user', selection: ${JSON.stringify(selection)}
      });
      return { ok: true, refusal: '' };
    } catch (e) {
      return { ok: false, refusal: (e && e.kind) ? String(e.kind) : JSON.stringify(e) };
    }
  })()`)
}

/**
 * The journal's own key (`seq`) for the file row named `name` in `opId`'s item tree,
 * read the way the dialog reads it (`get_operation_log_tree`). Waits out a summary
 * that's still building, and unfolds each folder row it meets, so it finds the file
 * however the tree compacts the copy's folders.
 */
async function treeItemSeq(page: TauriPage, opId: string, name: string): Promise<number> {
  let seq = -1
  await expect
    .poll(
      async () => {
        seq = await page.evaluate<number>(`(async function() {
          var expanded = [];
          for (var round = 0; round < 8; round++) {
            var view = await window.__TAURI_INTERNALS__.invoke('get_operation_log_tree', {
              operationId: ${JSON.stringify(opId)},
              request: { expanded: expanded, selection: null, cursor: null, offset: 0, limit: 200 }
            });
            if (!view || view.summary.kind !== 'built') return -1;
            var hit = view.rows.find(function(r) { return r.key.kind === 'item' && r.label === ${JSON.stringify(name)}; });
            if (hit) return hit.key.seq;
            var closed = view.rows.filter(function(r) { return r.expandable && !r.expanded; });
            if (closed.length === 0) return -1;
            expanded = expanded.concat(closed.map(function(r) { return r.key; }));
          }
          return -1;
        })()`)
        return seq >= 0
      },
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
  return seq
}

/** Cancels every operation the manager currently holds — the command the queue
 *  window's Cancel button calls. */
async function cancelEverything(page: TauriPage): Promise<void> {
  await page.evaluate(`(async function() {
    var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
    var ids = ops.map(function(o) { return o.operationId; });
    if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
  })()`)
}

// ── Driving the dialog ───────────────────────────────────────────────────────

/**
 * Opens the history dialog the way the View menu does, and waits until it has
 * FINISHED READING the journal.
 *
 * ❌ Not "the body element exists": the body is the dialog's frame, and it goes into
 * the DOM the moment the dialog mounts, while `openOperationLog()`'s
 * `get_recent_operation_log_entries` round trip is still in flight and the frame
 * holds a spinner. That window is not an occasional slow-machine thing — a probe
 * over 120 opens saw the frame before the list 120 times — so a caller that read a
 * row on the next round trip was racing that fetch on EVERY open, and won only
 * because the fetch usually answers first. On a loaded Linux shard it doesn't, and
 * the row reads back as an empty string.
 *
 * So the wait is on what the read PRODUCES: the list, or the notice that stands in
 * for it when the journal is empty or the read failed. All three are terminal, so
 * this can't hang on a legitimately empty log.
 */
async function openOperationLog(page: TauriPage): Promise<void> {
  await page.evaluate(`(function() {
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'execute-command', payload: { commandId: 'log.operationLog' }
    });
  })()`)
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(
          `document.querySelector('#operation-log-body .op-list, #operation-log-body .notice') !== null`,
        ),
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
}

/**
 * Puts the list's cursor on `opId`, which is what the detail pane, the tree, and the
 * action bar all follow, and waits until the detail pane shows it. Rows are found by
 * their `data-op-id` hook, ❌ never by position: operations started in the same second
 * have no defined order, and every reversal here journals an inverse of its own.
 */
async function showOperation(page: TauriPage, opId: string): Promise<void> {
  const id = JSON.stringify(opId)
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(`(function() {
          var item = document.querySelector('${LOG_DIALOG} .op-list [data-op-id="' + ${id} + '"]');
          var row = item && item.closest('.column-list-row');
          if (!row) return false;
          if (!document.getElementById('op-head-' + ${id})) row.click();
          return document.getElementById('op-head-' + ${id}) !== null;
        })()`),
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
}

/** Reads one thing off the detail pane of the shown operation `opId`, or `''`. */
function detailText(page: TauriPage, opId: string, selector: string): Promise<string> {
  return page.evaluate<string>(`(function() {
    var pane = document.querySelector('${LOG_DIALOG} section[aria-labelledby="op-head-' + ${JSON.stringify(opId)} + '"]');
    var el = pane && pane.querySelector(${JSON.stringify(selector)});
    return el ? el.textContent.trim() : '';
  })()`)
}

/** The shown operation's rollback-state badge ("Can roll back", "Rolling back", …). */
function rollbackBadge(page: TauriPage, opId: string): Promise<string> {
  return detailText(page, opId, '.op-badge-rollback')
}

/** The refusal line the shown operation's last press earned, or `''`. */
function refusalNotice(page: TauriPage, opId: string): Promise<string> {
  return detailText(page, opId, '.op-refusal')
}

/** The sentence the shown operation carries on sight (what became of its files, or why it can't be rolled back). */
function reasonNotice(page: TauriPage, opId: string): Promise<string> {
  return detailText(page, opId, '.op-reason')
}

/** "N of M selected", beside the action bar's button, or `''` while there's no count. */
function selectedCount(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `(function() { var c = document.querySelector('${LOG_DIALOG} .selected-count'); return c ? c.textContent.trim() : ''; })()`,
  )
}

/**
 * The action bar's controls for `opId`. `aria-describedby` is what ties each one
 * (the rollback press, and a running reversal's Pause / Resume / Cancel) to the
 * operation it acts on, so using it means a regression that drops the association
 * breaks this suite rather than only the a11y tests.
 */
function controlsFor(opId: string): string {
  // `~=` rather than `=`: the press also points at the sentence explaining it, so its
  // `aria-describedby` is a token LIST.
  return `${LOG_DIALOG} button[aria-describedby~="op-head-${opId}"]`
}

/** What every control for `opId` currently says, in DOM order, without its key hint. */
function controlLabels(page: TauriPage, opId: string): Promise<string[]> {
  const sel = JSON.stringify(controlsFor(opId))
  return page.evaluate<string[]>(
    `(function() {
        var out = [];
        var els = document.querySelectorAll(${sel});
        for (var i = 0; i < els.length; i++) {
          var copy = els[i].cloneNode(true);
          var hidden = copy.querySelectorAll('[aria-hidden="true"]');
          for (var j = 0; j < hidden.length; j++) hidden[j].remove();
          out.push((copy.textContent || '').trim());
        }
        return out;
    })()`,
  )
}

/** The word the running reversal shows for its live state ("Paused"), or `''` while it's moving. */
function pausedText(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `(function() { var b = document.querySelector('${LOG_DIALOG} .rollback-controls .paused-badge'); return b ? b.textContent.trim() : ''; })()`,
  )
}

/**
 * Presses one of the running reversal's controls by the word on it (Pause / Resume /
 * Cancel).
 *
 * Going through the real buttons rather than `pause_operation` over IPC is the
 * point: it pins the whole chain the dialog depends on (the journal's
 * `inverse_op_id` reaching the row, the controls binding a session to the INVERSE
 * operation rather than its own, and that session's command reaching the
 * engine's item-boundary gate). An IPC poke would pass with every one of those
 * links broken.
 */
async function pressControl(page: TauriPage, opId: string, label: string): Promise<void> {
  await clickButtonByText(page, controlsFor(opId), label)
}

/**
 * Shows `opId`, presses the action bar's rollback button, and waits for the question.
 * `label` is what that button SAYS, which is also the assertion: it reads "Roll back
 * selected (N)" only once the operation's tree has answered, so pressing by its full
 * words also waits for the tree, and a bar offering the wrong words for its state
 * can't be pressed here at all.
 */
async function pressRollBack(page: TauriPage, opId: string, label: string): Promise<void> {
  await showOperation(page, opId)
  await clickButtonByText(page, controlsFor(opId), label)
  await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: waitBudget(5000) }).toBe(1)
}

/**
 * Unchecks (or checks) the tree row named `name` by its own checkbox, the way a click
 * does. The box's accessible name is "Roll back <name>", which is also how a screen
 * reader finds it. Folders unfold until the row is listed, since a copy's top folder
 * may arrive closed.
 */
async function toggleTreeItem(page: TauriPage, name: string): Promise<void> {
  const label = JSON.stringify(`Roll back ${name}`)
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(`(function() {
          var box = document.querySelector('${LOG_DIALOG} [role="treegrid"] input[aria-label=' + JSON.stringify(${label}) + ']');
          if (box) { box.click(); return true; }
          var closed = document.querySelector('${LOG_DIALOG} [role="treegrid"] [aria-expanded="false"] .disclosure');
          if (closed) closed.click();
          return false;
        })()`),
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
}

/** The confirmation's body sentence — the wording that has to match the inverse. */
function confirmBody(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `(function() { var b = document.querySelector('#rollback-confirmation-body'); return b ? b.textContent.trim() : ''; })()`,
  )
}

/** The partial notice's "came back" sentence for `n` items. */
function backCountText(n: number): string {
  return n === 1 ? '1 item is back the way it was.' : `${String(n)} items are back the way they were.`
}

/** The partial notice's "still waiting" sentence for `n` items. */
function waitingCountText(n: number): string {
  return n === 1 ? '1 item hasn’t been rolled back yet.' : `${String(n)} items haven’t been rolled back yet.`
}

/** Answers the confirmation with its confirming button, and waits for it to close. */
async function confirmRollBack(page: TauriPage, label = 'Roll back'): Promise<void> {
  await clickButtonByText(page, `${CONFIRM_DIALOG} button`, label)
  await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: waitBudget(5000) }).toBe(0)
}

/** The confirmation's title — the sentence that frames what the press will do. */
function confirmTitle(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `(function() { var t = document.querySelector('#rollback-confirmation-title'); return t ? t.textContent.trim() : ''; })()`,
  )
}

/** Answers the confirmation with the safe choice, leaving the operation alone. */
async function declineRollBack(page: TauriPage): Promise<void> {
  await clickButtonByText(page, `${CONFIRM_DIALOG} button`, 'Leave it as is')
  await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: waitBudget(5000) }).toBe(0)
}

/** Closes the history dialog and opens it again, which re-reads page one. A reversal
 *  that ends before the dialog's controls subscribe to it (an unthrottled one can)
 *  leaves the open dialog with the header it had, so a test about an operation AFTER
 *  a reversal reopens it rather than racing that subscription. */
async function reopenOperationLog(page: TauriPage): Promise<void> {
  await escapeOverlayUntilGone(page, '.modal-overlay')
  await openOperationLog(page)
}

/** Sets (or with `null`, clears) the per-item rollback pause. */
async function setRollbackThrottle(page: TauriPage, ms: number | null): Promise<void> {
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('set_test_rollback_throttle', { ms: ${ms === null ? 'null' : String(ms)} })`,
  )
}

// ── Hooks ────────────────────────────────────────────────────────────────────

test.describe('Rolling an operation back from the history dialog', () => {
  // Every test stages a transfer, reverses it, and waits the reversal out. The
  // default 15 s budget covers the un-throttled tests but not the ones that pace
  // six items on purpose, and a per-test override would drift out of step.
  test.describe.configure({ timeout: waitBudget(45000) })

  test.beforeEach(async ({ tauriPage }) => {
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(tauriPage)
  })

  // ⚠️ ONE hook, and the order inside it is load-bearing (the trap
  // `operation-queue.spec.ts` documents): clear the throttle and drain every
  // operation FIRST, put the fixture tree back SECOND. The restore deletes the
  // `left/rb-*` dirs these tests create, and pulling a directory out from under a
  // still-running reversal leaves a retained failure that poisons the next test.
  // Split across two `afterEach`es the ordering is invisible, because Playwright
  // runs same-suite hooks in declaration order.
  test.afterEach(async ({ tauriPage }) => {
    await tauriPage.evaluate(`(async function() {
      try { await window.__TAURI_INTERNALS__.invoke('set_test_rollback_throttle', { ms: null }); } catch (e) {}
      try {
        var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
        var ids = ops.map(function(o) { return o.operationId; });
        if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
      } catch (e) {}
      for (var i = 0; i < 60; i++) {
        try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
        var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
        if (!remaining || remaining.length === 0) break;
        await new Promise(function(r) { setTimeout(r, 100); });
      }
    })()`)
    // Most tests end with the history dialog still open, and a staged transfer
    // raises its own completion toast; the leak guard fails on either. Escaping
    // until gone also takes the confirmation down if a test left one stacked.
    if ((await tauriPage.count('.modal-overlay')) > 0) {
      await escapeOverlayUntilGone(tauriPage, '.modal-overlay')
    }
    await dismissAllToasts(tauriPage)
    restoreFixtureTree(getFixtureRoot())
  })

  // ── The two happy paths ────────────────────────────────────────────────────

  test('undoing a copy deletes what the copy wrote', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-copy', 3)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-copy')
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-copy', 0))).toBe(true)

    await openOperationLog(page)
    // The operation is on the first page of history and says it can be reversed. ❌
    // Not "it's the top row": operations that start in the same second have no defined
    // order between them, and the reversal this suite runs journals its own inverse.
    await showOperation(page, opId)
    expect(await rollbackBadge(page, opId)).toBe('Can roll back')

    // Untouched, every file is checked, and the press says how many it takes.
    await pressRollBack(page, opId, 'Roll back selected (3)')
    expect(await confirmTitle(page)).toBe('Roll this operation back?')
    await confirmRollBack(page)

    // The badge flips under the cursor that pressed it, off the journal's own
    // synchronous write rather than optimism in the component. A three-file rollback
    // can finish before the first poll, and a finished one says "Rolled back", so
    // either word proves the flip.
    await expect
      .poll(() => rollbackBadge(page, opId), { timeout: waitBudget(5000) })
      .toMatch(/^(Rolling back|Rolled back)$/)

    expect(await settledRollbackState(page, opId)).toBe('rolledBack')
    expect(goneCount(fixtureRoot, 'rb-copy', 3)).toBe(3)
    // The copy's SOURCE is untouched: undoing a copy takes away only what it made.
    expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-copy', 0))).toBe(true)
    // And the directory the copy created goes with it, not only the files inside.
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'rb-copy'))).toBe(false)
  })

  test('undoing a move brings the files home', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-move', 3)

    const opId = await stageTransfer(page, 'move', fixtureRoot, 'rb-move')
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-move', 0))).toBe(true)
    expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-move', 0))).toBe(false)

    await openOperationLog(page)
    await showOperation(page, opId)
    expect(await rollbackBadge(page, opId)).toBe('Can roll back')
    // A local move renames its folder as one, so the folder is the one thing to roll back.
    await pressRollBack(page, opId, 'Roll back selected (1)')
    await confirmRollBack(page)

    expect(await settledRollbackState(page, opId)).toBe('rolledBack')
    for (let i = 0; i < 3; i++) {
      expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-move', i))).toBe(true)
      expect(fs.existsSync(destFile(fixtureRoot, 'rb-move', i))).toBe(false)
    }
  })

  // ── The question in front of it ────────────────────────────────────────────

  test('the question matches the inverse: undoing a move never mentions deleting', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-words-copy', 1)
    makeSourceDir(fixtureRoot, 'rb-words-move', 1)

    const copyId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-words-copy')
    const moveId = await stageTransfer(page, 'move', fixtureRoot, 'rb-words-move')

    await openOperationLog(page)

    // The move's question. A move's inverse restores and deletes nothing, and
    // scaring someone off it with delete language is the regression this catches.
    await pressRollBack(page, moveId, 'Roll back selected (1)')
    const moveWords = await confirmBody(page)
    await declineRollBack(page)
    expect(moveWords).toContain('moves the files back where they came from')
    expect(moveWords.toLowerCase()).not.toContain('delet')

    // The copy's question: this one DOES delete, and says so.
    await pressRollBack(page, copyId, 'Roll back selected (1)')
    const copyWords = await confirmBody(page)
    await declineRollBack(page)
    expect(copyWords.toLowerCase()).toContain('delet')
    // Two different questions, not one body reused for both.
    expect(copyWords).not.toBe(moveWords)

    // Declining changed nothing: both operations are still reversible, and every
    // file is still where the transfers left it.
    expect(await rollbackStateOf(page, moveId)).toBe('rollbackable')
    expect(await rollbackStateOf(page, copyId)).toBe('rollbackable')
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-words-move', 0))).toBe(true)
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-words-copy', 0))).toBe(true)
  })

  // ── The edge cases ─────────────────────────────────────────────────────────

  test('a destination changed since the copy is left alone, and the reversal reports itself partial', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-partial', 3)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-partial')

    // Someone edits one of the copies afterwards. The journal recorded its size, so
    // this one no longer matches what the operation says it wrote — and an undo
    // that deleted it would destroy work the operation never did.
    const edited = destFile(fixtureRoot, 'rb-partial', 1)
    const editedText = 'edited by the user after the copy finished'
    fs.writeFileSync(edited, editedText)

    await openOperationLog(page)
    await pressRollBack(page, opId, 'Roll back selected (3)')
    await confirmRollBack(page)

    expect(await settledRollbackState(page, opId)).toBe('partiallyRolledBack')
    expect(fs.existsSync(edited)).toBe(true)
    expect(fs.readFileSync(edited, 'utf8')).toBe(editedText)
    // The two it could prove are gone, so a partial is genuinely partial rather
    // than a reversal that gave up at the first surprise.
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-partial', 0))).toBe(false)
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-partial', 2))).toBe(false)
    // A directory that still holds something isn't removed either.
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'rb-partial'))).toBe(true)
  })

  test('cancelling a reversal keeps what it reversed, and the dialog offers to roll back the rest', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const count = 6
    makeSourceDir(fixtureRoot, 'rb-cancel', count)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-cancel')
    // Pace the reversal so there IS a midway to cancel at. Set after the copy, so
    // the copy itself ran at full speed.
    await setRollbackThrottle(page, ROLLBACK_THROTTLE_MS)

    await openOperationLog(page)
    await pressRollBack(page, opId, 'Roll back selected (6)')
    await confirmRollBack(page)

    // Wait on state, never on a clock: the moment one destination is gone the
    // reversal is demonstrably underway, and the next item is a throttle away.
    await expect
      .poll(() => goneCount(fixtureRoot, 'rb-cancel', count), { timeout: waitBudget(15000), intervals: [25] })
      .toBeGreaterThanOrEqual(1)

    // Cancel it with the command the queue window's Cancel button calls. That
    // button's own wiring is `operation-queue.spec.ts`'s job; what's untested until
    // here is that a REVERSAL is an ordinary managed operation the queue can stop.
    await cancelEverything(page)

    expect(await settledRollbackState(page, opId)).toBe('partiallyRolledBack')
    // What came back stays back, and what the cancel spared stays put. Both halves
    // matter: a cancel that undid its own work, and one that carried on to the end,
    // would each fail exactly one of these.
    const gone = goneCount(fixtureRoot, 'rb-cancel', count)
    expect(gone).toBeGreaterThanOrEqual(1)
    expect(gone).toBeLessThan(count)
    // Every source file is untouched throughout: cancelling the undo of a COPY can
    // never cost the user the original.
    for (let i = 0; i < count; i++) {
      expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-cancel', i))).toBe(true)
    }

    // ── And now the way out. Cancelling a reversal is an ordinary thing to do, and
    // the state it lands in used to be a dead end: a badge, no button, no words, and
    // the leftovers stranded at the destination with no path forward in the UI.
    await setRollbackThrottle(page, null)
    await reopenOperationLog(page)

    await showOperation(page, opId)
    expect(await rollbackBadge(page, opId)).toBe('Partly rolled back')
    // The detail says what became of the files without being asked, counted off the
    // tree once it answers: what came back, and what's still waiting.
    await expect
      .poll(() => reasonNotice(page, opId), { timeout: waitBudget(5000) })
      .toBe(`${backCountText(gone)} ${waitingCountText(count - gone)}`)

    // The button is worded by what THIS press does. `pressRollBack` clicks by text,
    // so a bar still offering a fresh "Roll back" here fails to find anything to press.
    await pressRollBack(page, opId, 'Roll back the rest')
    expect(await confirmTitle(page)).toBe('Roll back the rest?')
    await confirmRollBack(page, 'Roll back the rest')

    // The second pass re-streams every item and rechecks each against the journal:
    // the ones the first pass removed read as already gone and are credited without
    // acting, so the run has no skips and the operation lands FULLY reversed.
    expect(await settledRollbackState(page, opId)).toBe('rolledBack')
    expect(goneCount(fixtureRoot, 'rb-cancel', count)).toBe(count)
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'rb-cancel'))).toBe(false)
    // Still every source file, after two passes over them.
    for (let i = 0; i < count; i++) {
      expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-cancel', i))).toBe(true)
    }
  })

  // ── The refusals ───────────────────────────────────────────────────────────

  test('an operation that went stale under the dialog refuses, and says why', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-stale', 2)
    const label = 'Roll back selected (2)'

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-stale')
    await openOperationLog(page)
    await showOperation(page, opId)
    await expect.poll(() => controlLabels(page, opId), { timeout: waitBudget(5000) }).toContain(label)

    // Reverse it from somewhere else entirely (what an MCP client or Ask Cmdr does)
    // while the dialog holds its cached header. The bar on screen still offers a
    // press for an operation that has already been undone — the precise race the
    // refusal notice exists for, and the only one reachable from this dialog.
    expect((await dispatchRollback(page, opId)).ok).toBe(true)
    expect(await settledRollbackState(page, opId)).toBe('rolledBack')

    await pressRollBack(page, opId, label)
    await confirmRollBack(page)

    // Typed all the way: the backend answered `alreadyRolledBack`, and the detail
    // words that reason specifically rather than showing a generic apology or,
    // worse, letting the press look like it did nothing.
    await expect
      .poll(() => refusalNotice(page, opId), { timeout: waitBudget(5000) })
      .toBe('This one is already back the way it was.')
    // The button stays: every refusal here is a race the user can respond to.
    expect(await controlLabels(page, opId)).toContain(label)
  })

  test('the backend refuses an unknown, an already-rolling, and a never-rollbackable operation, each typed', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-refuse', 6)

    // 1. An operation that isn't in the history at all.
    expect((await dispatchRollback(page, 'no-such-operation')).refusal).toBe('unknownOperation')

    // 2. A second request while the first reversal is still running. From the UI
    //    this is only reachable as a race (the row's button goes away the moment
    //    it's pressed), but two clients can both hold a stale view, and the
    //    double-rollback guard is what stops one operation being reversed twice.
    const copyId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-refuse')
    await setRollbackThrottle(page, ROLLBACK_THROTTLE_MS)
    expect((await dispatchRollback(page, copyId)).ok).toBe(true)
    expect((await dispatchRollback(page, copyId)).refusal).toBe('alreadyRollingBack')
    await setRollbackThrottle(page, null)
    expect(await settledRollbackState(page, copyId)).toBe('rolledBack')

    // 3. An operation the journal recorded as unreversible. A permanent delete kept
    //    no copy of what it removed, so there's nothing to put back, and the
    //    refusal has to carry that reason rather than a bare "no".
    const doomed = path.join(fixtureRoot, 'left', 'rb-refuse-delete.txt')
    fs.writeFileSync(doomed, 'x'.repeat(FILE_BYTES))
    const beforeDelete = new Set((await recentEntries(page, 30)).map((r) => r.opId))
    await page.evaluate(`(async function() {
      await window.__TAURI_INTERNALS__.invoke('delete_files', {
        sources: [${JSON.stringify(doomed)}], volumeId: 'root', config: null, initiator: 'user'
      });
    })()`)
    let deleteId = ''
    await expect
      .poll(
        async () => {
          // By difference from the set taken before the delete: the reversal above
          // journals its own inverse operation, which is ALSO not rollbackable, so
          // matching on the state alone would happily pick that one up instead.
          const fresh = (await recentEntries(page, 30)).find((r) => !beforeDelete.has(r.opId))
          if (!fresh || fresh.executionStatus !== 'done' || fresh.rollbackState !== 'notRollbackable') return false
          deleteId = fresh.opId
          return true
        },
        { timeout: waitBudget(10000) },
      )
      .toBe(true)
    expect((await dispatchRollback(page, deleteId)).refusal).toBe('notRollbackable')
    expect(fs.existsSync(doomed)).toBe(false)

    // The reason has to survive the whole chain, because the DIALOG is where it
    // matters and no mock can prove that join: the engine has to store it, the row
    // has to carry it over the wire, and the detail has to say it on sight. This
    // operation never offers a press, so the refusal path above can't reach a user at
    // all — if the reason stopped being journaled or stopped being rendered, a person
    // would see "Can't roll back" and no explanation, and every test above would
    // still pass.
    const deleteRow = (await recentEntries(page, 30)).find((r) => r.opId === deleteId)
    expect(deleteRow?.notRollbackableReason).toBe('permanentDelete')
    await openOperationLog(page)
    await showOperation(page, deleteId)
    await expect
      .poll(() => reasonNotice(page, deleteId), { timeout: waitBudget(5000) })
      .toBe('A permanent delete leaves nothing to put back.')
    // And nothing to press.
    expect(await controlLabels(page, deleteId)).toEqual([])
  })

  // ── Progress, pause, and mid-file cancel, end to end ───────────────────────
  //
  // The engine's own suite proves these against in-memory volumes. These three
  // prove the wiring: that the sink built at the IPC edge reaches the inverse
  // operation, that `pause_operation` on the queue row parks the real engine, and
  // that a cancel inside one large file leaves the disk whole.

  /**
   * Per-item progress on the inverse operation, from a sink injected at the IPC
   * edge. The totals come off the journal before the first act, so the bar means
   * something from the first frame.
   */
  test('a reversal reports honest forward progress', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const count = 6
    makeSourceDir(fixtureRoot, 'rb-progress', count)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-progress')
    // No pacing here on purpose: what's under test is the shape of the frames, not
    // catching the reversal mid-flight, and the engine always sends the first frame
    // and the one that lands on the total however fast it runs.

    // Collect the reversal's own progress frames.
    await page.evaluate(`(async function() {
      window.__rbProgress = [];
      var handlerId = window.__TAURI_INTERNALS__.transformCallback(function(event) {
        window.__rbProgress.push(event.payload);
      });
      window.__rbProgressId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
        event: 'write-progress', target: { kind: 'Any' }, handler: handlerId,
      });
    })()`)

    await openOperationLog(page)
    await pressRollBack(page, opId, 'Roll back selected (6)')
    await confirmRollBack(page)
    expect(await settledRollbackState(page, opId)).toBe('rolledBack')

    // Forward, monotonic, and it reaches its total. A backwards bar is right on the
    // transfer dialog (it drains a bar already full); a reversal launched from
    // history opens a FRESH bar, where full would mean "nothing done yet".
    const frames = await page.evaluate<{ filesDone: number; filesTotal: number }[]>(
      // `WriteOperationPhase` goes over the wire snake_case (see `src/lib/ipc/bindings.ts`),
      // unlike most of our IPC enums.
      `(window.__rbProgress || []).filter(function(p) { return p.phase === 'rolling_back'; })`,
    )
    expect(frames.length).toBeGreaterThan(0)
    expect(frames[frames.length - 1]?.filesDone).toBe(count)
    for (let i = 1; i < frames.length; i++) {
      expect(frames[i]?.filesDone).toBeGreaterThanOrEqual(frames[i - 1]?.filesDone ?? 0)
    }

    await page.evaluate(`(async function() {
      if (window.__rbProgressId !== undefined) {
        await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', { event: 'write-progress', eventId: window.__rbProgressId });
      }
      delete window.__rbProgress; delete window.__rbProgressId;
    })()`)
  })

  /**
   * `pause_operation` on the inverse op reaches the engine's item-boundary gate.
   * A copy undo is all deletes, so nothing streams: this gate is the only thing
   * that can park it.
   */
  test('a paused reversal stops advancing, and resumes where it left off', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const count = 6
    makeSourceDir(fixtureRoot, 'rb-pause', count)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-pause')
    await setRollbackThrottle(page, PAUSE_THROTTLE_MS)

    await openOperationLog(page)
    await pressRollBack(page, opId, 'Roll back selected (6)')
    await confirmRollBack(page)

    await expect
      .poll(() => goneCount(fixtureRoot, 'rb-pause', count), { timeout: waitBudget(15000), intervals: [25] })
      .toBeGreaterThanOrEqual(1)

    // The reversal is live and named on the wire as one. Every surface that shows
    // it (queue row, corner chip, progress dialog) words itself off THIS field, so
    // a mock can't pin it: the wording tests would all still pass while the app
    // silently went back to titling a restore "Rolling back…". Read in the same
    // `list_operations` round trip the pause below needs anyway, so it costs no
    // extra time. `reverses` is the ORIGINAL kind, and this reversal undoes a copy.
    const reverses = await page.evaluate(`(async function() {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      return ops.map(function(op) { return op.reverses });
    })()`)
    expect(reverses).toContain('copy')

    // From the dialog's own action bar, which is the whole affordance: the dialog
    // that started this reversal is where a person reaches for the brake.
    await pressControl(page, opId, 'Pause')

    // Held: the count stops moving across several throttle windows. This is the one
    // place where waiting IS the assertion, so it's a generous multiple of the
    // per-item pause rather than a guess.
    const held = goneCount(fixtureRoot, 'rb-pause', count)
    await new Promise((resolve) => setTimeout(resolve, PAUSE_THROTTLE_MS * 4))
    expect(goneCount(fixtureRoot, 'rb-pause', count)).toBe(held)

    // ❌ A parked reversal must not read as a finished one: the bar says so, and
    // the button has turned into its own undo.
    expect(await controlLabels(page, opId)).toContain('Resume')
    await expect.poll(() => pausedText(page), { timeout: waitBudget(5000) }).toContain('Paused')

    await pressControl(page, opId, 'Resume')

    // Resumed from where it parked rather than restarted: it finishes the whole set.
    expect(await settledRollbackState(page, opId)).toBe('rolledBack')
    expect(goneCount(fixtureRoot, 'rb-pause', count)).toBe(count)
  })

  /**
   * Whichever side holds the file holds all of it. A local→local move restores by
   * rename, so this is the wiring case rather than the streaming one; the mid-file
   * stop itself is pinned in Rust, where a chunk delay opens a real window
   * (`control_tests.rs::stopping_inside_one_large_file_leaves_no_partial_and_loses_nothing`).
   *
   * The body needs a LARGE file to have any window at all, hence
   * `left/bulk/large-1.dat` rather than the 1 KB fixtures the rest of the suite
   * uses. `bulk/` survives `recreateFixtures`, so copying from it is cheap.
   */
  test('cancelling inside one large file leaves no partial behind and loses nothing', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const dir = path.join(fixtureRoot, 'left', 'rb-midfile')
    fs.mkdirSync(dir, { recursive: true })
    const big = path.join(dir, 'f-0.txt')
    fs.copyFileSync(path.join(fixtureRoot, 'left', 'bulk', 'large-1.dat'), big)
    const sourceBytes = fs.statSync(big).size

    const opId = await stageTransfer(page, 'move', fixtureRoot, 'rb-midfile')
    const dest = destFile(fixtureRoot, 'rb-midfile', 0)

    await openOperationLog(page)
    await pressRollBack(page, opId, 'Roll back selected (1)')
    await confirmRollBack(page)

    // Cancel while the single file is still travelling back.
    await expect.poll(() => fs.existsSync(big), { timeout: waitBudget(15000), intervals: [25] }).toBe(true)
    await cancelEverything(page)
    await expect
      .poll(async () => (await rollbackStateOf(page, opId)) !== 'rollingBack', { timeout: waitBudget(20000) })
      .toBe(true)

    // Whichever side holds the file, it holds ALL of it, and no half-written copy
    // survives on the other. A truncated file at either end is the failure.
    const restored = fs.existsSync(big)
    const stillAtDest = fs.existsSync(dest)
    expect(restored || stillAtDest).toBe(true)
    if (restored) expect(fs.statSync(big).size).toBe(sourceBytes)
    if (stillAtDest) expect(fs.statSync(dest).size).toBe(sourceBytes)
  })

  // ── Rolling back part of an operation ─────────────────────────────────────
  //
  // Which rows a selection takes, and that a skipped or unchecked one keeps the
  // operation partial, are pinned in Rust (`crates/cmdr-operation-log/src/tree/selection_tests.rs`
  // and `crates/cmdr-operations/src/rollback/planner_tests/selection_tests.rs`). These two prove the wiring: a checkbox in a real
  // webview narrows what the real engine does to real files, and "the rest" picks
  // up exactly what a subset left.

  test('unchecking one file rolls back only the others, and the operation reads partial', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-subset', 3)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-subset')
    await openOperationLog(page)
    await showOperation(page, opId)
    // Untouched, all three are checked.
    await expect
      .poll(() => controlLabels(page, opId), { timeout: waitBudget(5000) })
      .toContain('Roll back selected (3)')

    await toggleTreeItem(page, 'f-1.txt')
    await expect.poll(() => selectedCount(page), { timeout: waitBudget(5000) }).toBe('2 of 3 selected')

    // The bar, the question, and its button all say "selected" for a narrowed press.
    await pressRollBack(page, opId, 'Roll back selected (2)')
    expect(await confirmTitle(page)).toBe('Roll back the selected items?')
    expect(await confirmBody(page)).toContain('deletes the selected files')
    await confirmRollBack(page, 'Roll back selected')

    // Exactly the two checked copies are gone, the unchecked one stays, and every
    // original is untouched. The folder holds something still, so it stays too.
    expect(await settledRollbackState(page, opId)).toBe('partiallyRolledBack')
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-subset', 0))).toBe(false)
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-subset', 1))).toBe(true)
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-subset', 2))).toBe(false)
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'rb-subset'))).toBe(true)
    for (let i = 0; i < 3; i++) expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-subset', i))).toBe(true)

    // The operation says what it is now: partly back, counted, with the rest on offer.
    await reopenOperationLog(page)
    await showOperation(page, opId)
    expect(await rollbackBadge(page, opId)).toBe('Partly rolled back')
    await expect
      .poll(() => reasonNotice(page, opId), { timeout: waitBudget(5000) })
      .toBe(`${backCountText(2)} ${waitingCountText(1)}`)
    expect(await controlLabels(page, opId)).toContain('Roll back the rest')
  })

  test('after a subset from elsewhere, "Roll back the rest" takes what it left', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-rest', 3)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-rest')
    // A subset straight through the engine command (what an MCP client or Ask Cmdr
    // sends): everything but `f-1.txt`, keyed by the journal's own row.
    const seq = await treeItemSeq(page, opId, 'f-1.txt')
    expect(
      (await dispatchRollback(page, opId, { base: 'everything', toggles: [{ kind: 'item', seq, on: false }] })).ok,
    ).toBe(true)
    expect(await settledRollbackState(page, opId)).toBe('partiallyRolledBack')
    expect(goneCount(fixtureRoot, 'rb-rest', 3)).toBe(2)
    expect(fs.existsSync(destFile(fixtureRoot, 'rb-rest', 1))).toBe(true)

    // The dialog offers the rest, and the rest is the one file the subset left.
    await openOperationLog(page)
    await pressRollBack(page, opId, 'Roll back the rest')
    expect(await confirmTitle(page)).toBe('Roll back the rest?')
    await confirmRollBack(page, 'Roll back the rest')

    expect(await settledRollbackState(page, opId)).toBe('rolledBack')
    expect(goneCount(fixtureRoot, 'rb-rest', 3)).toBe(3)
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'rb-rest'))).toBe(false)
    for (let i = 0; i < 3; i++) expect(fs.existsSync(sourceFile(fixtureRoot, 'rb-rest', i))).toBe(true)
  })
})
