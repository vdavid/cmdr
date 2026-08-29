/**
 * E2E for rolling an operation back from the history dialog: the Roll back button,
 * the question it asks, and what actually happens to the files on disk.
 *
 * **What this suite is for, and what it leaves to other layers.** The engine's
 * semantics (per-kind inverses, the snapshot recheck, the never-overwrite restore,
 * the typed skip reasons) are pinned in Rust against in-memory volumes, in
 * `src-tauri/src/operation_log/rollback/tests.rs` — cheaply and exhaustively.
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
 *   against in-memory volumes (`operation_log/rollback/control_tests.rs`) with a
 *   real mid-file window. The three at the bottom here prove the same three reach a
 *   real webview, a real queue row, and real files on a real disk.
 *
 * Requires `--features playwright-e2e`.
 */

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
          config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] }
        })`
      : `window.__TAURI_INTERNALS__.invoke('move_files', {
          sources: [${src}], destination: ${destDir},
          config: { conflictResolution: 'rename', progressIntervalMs: 100, maxConflictsToShow: 10, previewId: null, preKnownConflicts: [] },
          initiator: null
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
      { timeout: 10000 },
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
      timeout: 20000,
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
function dispatchRollback(page: TauriPage, opId: string): Promise<{ ok: boolean; refusal: string }> {
  return page.evaluate<{ ok: boolean; refusal: string }>(`(async function() {
    try {
      await window.__TAURI_INTERNALS__.invoke('rollback_operation', { operationId: ${JSON.stringify(opId)} });
      return { ok: true, refusal: '' };
    } catch (e) {
      return { ok: false, refusal: (e && e.kind) ? String(e.kind) : JSON.stringify(e) };
    }
  })()`)
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

/** Opens the history dialog the way the View menu does, and waits for it. */
async function openOperationLog(page: TauriPage): Promise<void> {
  await page.evaluate(`(function() {
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'execute-command', payload: { commandId: 'log.operationLog' }
    });
  })()`)
  await expect
    .poll(() => page.evaluate<boolean>(`document.querySelector('#operation-log-body') !== null`), { timeout: 5000 })
    .toBe(true)
}

/** The rollback-state badge on the row for `opId` ("Can roll back", "Rolling back", …). */
function rowRollbackBadge(page: TauriPage, opId: string): Promise<string> {
  return page.evaluate<string>(`(function() {
    var head = document.getElementById('op-head-' + ${JSON.stringify(opId)});
    var badge = head && head.querySelector('.op-badge-rollback');
    return badge ? badge.textContent.trim() : '';
  })()`)
}

/** The refusal line under the row for `opId`, or `''` when there is none. */
function rowRefusalNotice(page: TauriPage, opId: string): Promise<string> {
  return page.evaluate<string>(`(function() {
    var head = document.getElementById('op-head-' + ${JSON.stringify(opId)});
    var li = head && head.closest('li.op');
    var notice = li && li.querySelector('.op-refusal');
    return notice ? notice.textContent.trim() : '';
  })()`)
}

/** The explanatory line a not-rollbackable row carries on sight (no button to press). */
function rowReasonNotice(page: TauriPage, opId: string): Promise<string> {
  return page.evaluate<string>(`(function() {
    var head = document.getElementById('op-head-' + ${JSON.stringify(opId)});
    var li = head && head.closest('li.op');
    var notice = li && li.querySelector('.op-reason');
    return notice ? notice.textContent.trim() : '';
  })()`)
}

/**
 * The row's own Roll back button. `aria-describedby` is what ties one to its row
 * (every button carries the same words), so it's also the only per-row handle a
 * test has — and using it means a regression that drops the association breaks
 * this suite rather than only the a11y tests.
 */
function rollBackButtonFor(opId: string): string {
  return `${LOG_DIALOG} button[aria-describedby="op-head-${opId}"]`
}

/** Whether the row for `opId` still offers a Roll back button. */
async function rowHasRollBackButton(page: TauriPage, opId: string): Promise<boolean> {
  return (await page.count(rollBackButtonFor(opId))) > 0
}

/** Presses the Roll back button on `opId`'s row, and waits for the question. */
async function pressRollBack(page: TauriPage, opId: string): Promise<void> {
  await clickButtonByText(page, rollBackButtonFor(opId), 'Roll back')
  await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: 5000 }).toBe(1)
}

/** The confirmation's body sentence — the wording that has to match the inverse. */
function confirmBody(page: TauriPage): Promise<string> {
  return page.evaluate<string>(
    `(function() { var b = document.querySelector('#rollback-confirmation-body'); return b ? b.textContent.trim() : ''; })()`,
  )
}

/** Answers the confirmation with "Roll back", and waits for it to close. */
async function confirmRollBack(page: TauriPage): Promise<void> {
  await clickButtonByText(page, `${CONFIRM_DIALOG} button`, 'Roll back')
  await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: 5000 }).toBe(0)
}

/** Answers the confirmation with the safe choice, leaving the operation alone. */
async function declineRollBack(page: TauriPage): Promise<void> {
  await clickButtonByText(page, `${CONFIRM_DIALOG} button`, 'Leave it as is')
  await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: 5000 }).toBe(0)
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
  test.describe.configure({ timeout: 45000 })

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
    // The row is on the first page of history and says it can be reversed. ❌ Not
    // "it's the top row": operations that start in the same second have no defined
    // order between them, and the reversal this suite runs journals its own inverse.
    expect(await rowRollbackBadge(page, opId)).toBe('Can roll back')

    await pressRollBack(page, opId)
    await confirmRollBack(page)

    // The badge flips under the cursor that pressed it, off the journal's own
    // synchronous write rather than optimism in the component.
    await expect.poll(() => rowRollbackBadge(page, opId), { timeout: 5000 }).toBe('Rolling back')

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
    expect(await rowRollbackBadge(page, opId)).toBe('Can roll back')
    await pressRollBack(page, opId)
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
    await pressRollBack(page, moveId)
    const moveWords = await confirmBody(page)
    await declineRollBack(page)
    expect(moveWords).toContain('moves the files back where they came from')
    expect(moveWords.toLowerCase()).not.toContain('delet')

    // The copy's question, one row down: this one DOES delete, and says so.
    await pressRollBack(page, copyId)
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
    await pressRollBack(page, opId)
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

  test('cancelling a reversal keeps what it already reversed and leaves the rest', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const count = 6
    makeSourceDir(fixtureRoot, 'rb-cancel', count)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-cancel')
    // Pace the reversal so there IS a midway to cancel at. Set after the copy, so
    // the copy itself ran at full speed.
    await setRollbackThrottle(page, ROLLBACK_THROTTLE_MS)

    await openOperationLog(page)
    await pressRollBack(page, opId)
    await confirmRollBack(page)

    // Wait on state, never on a clock: the moment one destination is gone the
    // reversal is demonstrably underway, and the next item is a throttle away.
    await expect
      .poll(() => goneCount(fixtureRoot, 'rb-cancel', count), { timeout: 15000, intervals: [25] })
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
  })

  // ── The refusals ───────────────────────────────────────────────────────────

  test('a row that went stale under the dialog refuses, and says why', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    makeSourceDir(fixtureRoot, 'rb-stale', 2)

    const opId = await stageTransfer(page, 'copy', fixtureRoot, 'rb-stale')
    await openOperationLog(page)
    expect(await rowHasRollBackButton(page, opId)).toBe(true)

    // Reverse it from somewhere else entirely (what an MCP client or Ask Cmdr does)
    // while the dialog holds its cached header. The row on screen still offers a
    // button for an operation that has already been undone — the precise race the
    // refusal notice exists for, and the only one reachable from this dialog.
    expect((await dispatchRollback(page, opId)).ok).toBe(true)
    expect(await settledRollbackState(page, opId)).toBe('rolledBack')

    await clickButtonByText(page, rollBackButtonFor(opId), 'Roll back')
    await expect.poll(() => page.count(CONFIRM_DIALOG), { timeout: 5000 }).toBe(1)
    await confirmRollBack(page)

    // Typed all the way: the backend answered `alreadyRolledBack`, and the row
    // words that reason specifically rather than showing a generic apology or,
    // worse, letting the press look like it did nothing.
    await expect
      .poll(() => rowRefusalNotice(page, opId), { timeout: 5000 })
      .toBe('This one is already back the way it was.')
    // The button stays: every refusal here is a race the user can respond to.
    expect(await rowHasRollBackButton(page, opId)).toBe(true)
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
        sources: [${JSON.stringify(doomed)}], volumeId: 'root', config: null, initiator: null
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
        { timeout: 10000 },
      )
      .toBe(true)
    expect((await dispatchRollback(page, deleteId)).refusal).toBe('notRollbackable')
    expect(fs.existsSync(doomed)).toBe(false)

    // The reason has to survive the whole chain, because the DIALOG is where it
    // matters and no mock can prove that join: the engine has to store it, the row
    // has to carry it over the wire, and the row has to say it on sight. This row
    // never offers a button, so the refusal path above can't reach a user at all —
    // if the reason stopped being journaled or stopped being rendered, a person
    // would see "Can't roll back" and no explanation, and every test above would
    // still pass.
    const deleteRow = (await recentEntries(page, 30)).find((r) => r.opId === deleteId)
    expect(deleteRow?.notRollbackableReason).toBe('permanentDelete')
    await openOperationLog(page)
    await expect
      .poll(() => rowReasonNotice(page, deleteId), { timeout: 5000 })
      .toBe('A permanent delete leaves nothing to put back.')
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
    await pressRollBack(page, opId)
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
    await pressRollBack(page, opId)
    await confirmRollBack(page)

    await expect
      .poll(() => goneCount(fixtureRoot, 'rb-pause', count), { timeout: 15000, intervals: [25] })
      .toBeGreaterThanOrEqual(1)

    await page.evaluate(`(async function() {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      for (var i = 0; i < ops.length; i++) {
        await window.__TAURI_INTERNALS__.invoke('pause_operation', { operationId: ops[i].operationId });
      }
    })()`)

    // Held: the count stops moving across several throttle windows. This is the one
    // place where waiting IS the assertion, so it's a generous multiple of the
    // per-item pause rather than a guess.
    const held = goneCount(fixtureRoot, 'rb-pause', count)
    await new Promise((resolve) => setTimeout(resolve, PAUSE_THROTTLE_MS * 4))
    expect(goneCount(fixtureRoot, 'rb-pause', count)).toBe(held)

    await page.evaluate(`(async function() {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      for (var i = 0; i < ops.length; i++) {
        await window.__TAURI_INTERNALS__.invoke('resume_operation', { operationId: ops[i].operationId });
      }
    })()`)

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
    await pressRollBack(page, opId)
    await confirmRollBack(page)

    // Cancel while the single file is still travelling back.
    await expect.poll(() => fs.existsSync(big), { timeout: 15000, intervals: [25] }).toBe(true)
    await cancelEverything(page)
    await expect.poll(async () => (await rollbackStateOf(page, opId)) !== 'rollingBack', { timeout: 20000 }).toBe(true)

    // Whichever side holds the file, it holds ALL of it, and no half-written copy
    // survives on the other. A truncated file at either end is the failure.
    const restored = fs.existsSync(big)
    const stillAtDest = fs.existsSync(dest)
    expect(restored || stillAtDest).toBe(true)
    if (restored) expect(fs.statSync(big).size).toBe(sourceBytes)
    if (stillAtDest) expect(fs.statSync(dest).size).toBe(sourceBytes)
  })
})
