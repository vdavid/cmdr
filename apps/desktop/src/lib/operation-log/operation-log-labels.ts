/**
 * Client-side labels for the operation-log dialog, plus the two other things the
 * dialog derives from a typed enum: which rollback confirmation to raise, and which
 * sentence to show when a rollback is refused.
 *
 * Every one of them comes from a TYPED enum field, never a display string the
 * backend rendered: the per-operation summary
 * ("Moved 214 items") is formatted here from `kind` + a count via an ICU
 * plural key, so it localizes per viewer and shows a thousands separator. Status,
 * kind, initiator, and item-outcome labels map their enum to a catalog key with
 * an exhaustive switch (a new variant is a compile error until it's mapped).
 */

import type {
  Initiator,
  NotRollbackableReason,
  OpKind,
  ArchiveSubkind,
  ExecutionStatus,
  OperationRow,
  RollbackRefusal,
  RollbackState,
  SearchCoverageReason,
  SelectionRefusal,
  TreeTally,
} from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { rollbackConfirmVariant } from '#lib/file-operations/reversal-wording.js'

function summaryKey(kind: OpKind, subkind: ArchiveSubkind | null): MessageKey {
  switch (kind) {
    case 'copy':
      return 'operationLog.summary.copy'
    case 'move':
      return 'operationLog.summary.move'
    case 'delete':
      return 'operationLog.summary.delete'
    case 'trash':
      return 'operationLog.summary.trash'
    case 'rename':
      return 'operationLog.summary.rename'
    case 'createFolder':
      return 'operationLog.summary.createFolder'
    case 'createFile':
      return 'operationLog.summary.createFile'
    case 'tag':
      return 'operationLog.summary.tag'
    case 'archiveEdit':
      switch (subkind) {
        case 'compress':
          return 'operationLog.summary.compress'
        case 'extract':
          return 'operationLog.summary.archiveExtract'
        // A zip-inner edit, or a subkind the backend didn't record.
        case 'edit':
        case null:
          return 'operationLog.summary.archiveEdit'
      }
  }
}

/** The journal facts a summary is made of, all typed fields of `OperationRow`. */
export type SummaryFacts = Pick<OperationRow, 'kind' | 'archiveSubkind' | 'itemCount' | 'itemsDone' | 'executionStatus'>

/**
 * The one-line summary of an operation ("Moved 214 items"), formatted from the
 * typed `kind` (+ archive subkind) and a count. The plural form and the
 * thousands separator both follow the active locale.
 *
 * An operation that ended early (canceled, or stopped on a problem) counts what it
 * got done (`itemsDone`) and says it ended early, with the plan beside it: "Copied
 * 30 items, then canceled (1,200 planned)". Reading the planned `itemCount` there
 * claimed a whole copy that never happened. A finished or still-running one counts
 * its plan.
 */
export function operationSummary(op: SummaryFacts): string {
  const counted = (count: number) =>
    tString(summaryKey(op.kind, op.archiveSubkind), { count, countText: formatInteger(count) })
  switch (op.executionStatus) {
    case 'queued':
    case 'running':
    case 'done':
      return counted(op.itemCount)
    case 'canceled':
    case 'failed': {
      const key = op.executionStatus === 'canceled' ? 'operationLog.summary.canceled' : 'operationLog.summary.stopped'
      return tString(key, {
        summary: counted(Math.min(op.itemsDone, op.itemCount)),
        plannedText: formatInteger(op.itemCount),
      })
    }
  }
}

/**
 * Who started the operation: you, an external AI client (over MCP), or the agent. `agentEdited`
 * is mixed provenance: the agent proposed the batch and you retyped at least one name while
 * reviewing it, so the log doesn't credit the agent for names you chose.
 */
export function initiatorLabel(initiator: Initiator): string {
  switch (initiator) {
    case 'user':
      return tString('operationLog.initiator.user')
    case 'aiClient':
      return tString('operationLog.initiator.aiClient')
    case 'agent':
      return tString('operationLog.initiator.agent')
    case 'agentEdited':
      return tString('operationLog.initiator.agentEdited')
  }
}

/** The operation's lifecycle state. Style guide: no "failed" in copy. */
export function executionStatusLabel(status: ExecutionStatus): string {
  switch (status) {
    case 'queued':
      return tString('operationLog.status.queued')
    case 'running':
      return tString('operationLog.status.running')
    case 'done':
      return tString('operationLog.status.finished')
    case 'failed':
      return tString('operationLog.status.failed')
    case 'canceled':
      return tString('operationLog.status.canceled')
  }
}

/** Whether and how the operation can be, or has been, reversed. */
export function rollbackStateLabel(state: RollbackState): string {
  switch (state) {
    case 'notRollbackable':
      return tString('operationLog.rollback.notRollbackable')
    case 'rollbackable':
      return tString('operationLog.rollback.rollbackable')
    case 'rollingBack':
      return tString('operationLog.rollback.rollingBack')
    case 'rolledBack':
      return tString('operationLog.rollback.rolledBack')
    case 'partiallyRolledBack':
      return tString('operationLog.rollback.partiallyRolledBack')
  }
}

/**
 * What the row's own button offers, or `null` when the row offers nothing to press.
 *
 * `rest` is a `partiallyRolledBack` row picking a reversal back up. The engine
 * admits that state through the same gate as `rollbackable`
 * (`crates/cmdr-operation-log/src/rollback.rs`, `check_rollbackable`) and re-attempts every item
 * an earlier pass held back, with a fresh recheck (`RollbackUnit::awaits_reversal`).
 * ❌ Never widen this to a state the backend gate
 * refuses: the row would offer a press that can only come back as a refusal.
 */
export type RowRollbackAction = 'start' | 'rest'

export function rowRollbackAction(state: RollbackState): RowRollbackAction | null {
  switch (state) {
    case 'rollbackable':
      return 'start'
    case 'partiallyRolledBack':
      return 'rest'
    // Nothing to reverse, or a reversal is already running.
    case 'notRollbackable':
    case 'rollingBack':
    case 'rolledBack':
      return null
  }
}

/**
 * The button's words when no item tree narrows the press (the transfer dialog's
 * Undo, an operation whose tree can't be read yet): starting a reversal and picking
 * a stopped one back up are different promises, and "Roll back" on an operation
 * already half reversed would be making the wrong one.
 */
export function rowRollbackActionLabel(action: RowRollbackAction): string {
  switch (action) {
    case 'start':
      return tString('operationLog.dialog.rollBack')
    case 'rest':
      return tString('operationLog.dialog.rollBackRest')
  }
}

/** The facts of an op's item tree the action bar reads: the "N of M" and why 0 of 0 may still have work. */
export type ActionTally = Pick<TreeTally, 'selected' | 'selectable' | 'housekeepingWaiting'>

/**
 * What the action bar's button does, as `rollbackAction` decides it.
 *
 * - `all`: "Roll back", the whole operation, sending no selection.
 * - `rest`: "Roll back the rest" of a partly reversed one, sending no selection.
 * - `selected`: "Roll back selected (N)". It sends the tree's selection only when the
 *   person `narrowed` it; untouched, every check is on and no selection says the same.
 *   `subset` words the confirmation ("the selected items") when it leaves some out.
 */
export type RollbackAction =
  | { kind: 'all' }
  | { kind: 'rest' }
  | { kind: 'selected'; count: number; rest: boolean; subset: boolean }

/**
 * The action bar's press, or `null` on a state the backend gate refuses
 * (`rowRollbackAction`, ❌ never wider).
 *
 * `tally` is `null` while the tree can't be read (an old operation backfilling, the
 * journal building its indexes): the press then works exactly as it did before there
 * was a tree, on everything. `disabled` is only ever set at "0 of M" with M > 0, the
 * one state a press can't help; the engine's `nothingSelected` refusal is the backstop.
 *
 * ❌ No dead end: at "0 of 0" with rows still waiting (a copied folder held back, now
 * empty), `rollbackable` offers "Roll back" and `partiallyRolledBack` "Roll back the
 * rest", both sending no selection, which sweeps the folders that can go.
 */
export function rollbackAction(
  state: RollbackState,
  tally: ActionTally | null,
  narrowed: boolean,
): { action: RollbackAction; disabled: boolean } | null {
  const base = rowRollbackAction(state)
  if (base === null) return null
  const whole: RollbackAction = base === 'start' ? { kind: 'all' } : { kind: 'rest' }
  if (tally === null || tally.selectable === 0) return { action: whole, disabled: false }
  // A partly reversed op the person hasn't narrowed rolls back "the rest", never a fresh
  // reversal, whatever its count.
  if (base === 'rest' && !narrowed) return { action: whole, disabled: false }
  return {
    action: {
      kind: 'selected',
      count: tally.selected,
      rest: base === 'rest',
      subset: narrowed && tally.selected < tally.selectable,
    },
    disabled: tally.selected === 0,
  }
}

/** The action bar button's words. */
export function rollbackActionLabel(action: RollbackAction): string {
  switch (action.kind) {
    case 'all':
      return tString('operationLog.dialog.rollBack')
    case 'rest':
      return tString('operationLog.dialog.rollBackRest')
    case 'selected':
      return tString('operationLog.dialog.rollBackSelected', {
        count: action.count,
        countText: formatInteger(action.count),
      })
  }
}

/** "3 of 12 selected", beside the action bar's button. */
export function selectedCountText(tally: Pick<TreeTally, 'selected' | 'selectable'>): string {
  return tString('operationLog.dialog.selectedCount', {
    count: tally.selectable,
    selectedText: formatInteger(tally.selected),
    selectableText: formatInteger(tally.selectable),
  })
}

/**
 * The sentence an operation carries ON SIGHT, with no press to earn it, or `null` when
 * the badge says everything there is to say.
 *
 * - `notRollbackable`: its stored reason. It never offers the button whose refusal
 *   would otherwise carry the reason, so this is the only way it reaches the reader.
 * - `rollbackable`: what rolling back DOES, worded by the same reversal the
 *   confirmation names (`rollbackConfirmVariant`), so the checkboxes read as "these
 *   come back" or "these go".
 * - `partiallyRolledBack`: what became of the files. This is the fallback sentence;
 *   the dialog says it with counts when the item tree has them
 *   (`partiallyRolledBackSummary`).
 *
 * Exhaustive over `RollbackState`, so a new state has to decide whether it explains
 * itself rather than falling silent by default.
 */
export function rowStandingNotice(
  state: RollbackState,
  reason: NotRollbackableReason | null,
  kind: OpKind,
): MessageKey | null {
  switch (state) {
    case 'notRollbackable':
      // A NULL reason is an operation still running, which opens `not_rollbackable`
      // until finalize decides. A dangling label would be worse than silence.
      return reason === null ? null : notRollbackableNotice(reason)
    case 'rollbackable':
      return rollbackableNotice(kind)
    case 'partiallyRolledBack':
      return 'operationLog.rollback.partiallyRolledBackNotice'
    case 'rollingBack':
    case 'rolledBack':
      return null
  }
}

/** What rolling back an operation of this kind does to the checked items. */
function rollbackableNotice(kind: OpKind): MessageKey {
  // A copy's reversal deletes, like a new folder's, but only a copy has originals
  // elsewhere to reassure someone about.
  if (kind === 'copy') return 'operationLog.rollback.rollbackableNoticeCopy'
  switch (rollbackConfirmVariant(kind)) {
    case 'undoByDeleting':
    case 'stopAndDelete':
      return 'operationLog.rollback.rollbackableNoticeDelete'
    case 'undoByMovingBack':
    case 'stopAndMoveBack':
      return 'operationLog.rollback.rollbackableNoticeMoveBack'
    case 'undoByRenamingBack':
      return 'operationLog.rollback.rollbackableNoticeRenameBack'
    case 'undoByRestoringTags':
      return 'operationLog.rollback.rollbackableNoticeTagsBack'
  }
}

/**
 * A partly reversed operation's notice from the item tree's counts ("4 items are back
 * the way they were. Cmdr skipped 2 items it wasn't sure about. 2 items haven't been
 * rolled back yet."), or `null` when there's nothing to count, which leaves the fixed
 * sentence of {@link rowStandingNotice}.
 *
 * ❗ "haven't been rolled back yet" covers two things the journal can't tell apart: an
 * item someone left unchecked, and one a cancelled pass never reached. Both wait for the
 * next pass, and one honest phrase is better than a guess at which.
 */
export function partiallyRolledBackSummary(
  tally: Pick<TreeTally, 'rolledBack' | 'heldBack' | 'selectable'>,
): string | null {
  const waiting = Math.max(0, tally.selectable - tally.heldBack)
  const parts: string[] = []
  const counted = (key: MessageKey, count: number) => {
    if (count > 0) parts.push(tString(key, { count, countText: formatInteger(count) }))
  }
  counted('operationLog.rollback.partialRolledBack', tally.rolledBack)
  counted('operationLog.rollback.partialHeldBack', tally.heldBack)
  counted('operationLog.rollback.partialWaiting', waiting)
  return parts.length === 0 ? null : parts.join(' ')
}

/**
 * A second sentence about what the journal holds of an operation, or `null` when it
 * holds all of it (as far as rolling back cares). Only an operation a crash cut off
 * speaks: the rows it never wrote are rows a rollback can't reach, and the tree shows
 * only what was recorded. The other reasons are about name search coverage (an index
 * that wasn't live, a capped folder), which the tree already words per folder or which
 * a rollback doesn't depend on.
 */
export function coverageNotice(reason: SearchCoverageReason | null): MessageKey | null {
  switch (reason) {
    case 'interrupted':
      return 'operationLog.rollback.interruptedNotice'
    case 'capped':
    case 'indexAbsent':
    case 'indexStale':
    case 'volumeNotLive':
    case 'searchRowIncomplete':
    case null:
      return null
  }
}

/**
 * The sentence for a refused rollback. `null` covers the press that never reached the
 * backend at all, which carries no reason to report.
 *
 * The button only shows on a row the journal calls rollbackable, so every reason here
 * is a race the user lost: another window, the agent, or an external AI client got
 * there first, or a drive left. Each earns its own words, because "it can't be rolled
 * back" and "it already was" ask for different next moves.
 */
export function rollbackRefusalNotice(refusal: RollbackRefusal | null): MessageKey {
  if (refusal === null) return 'operationLog.rollback.refusalUnexpected'
  switch (refusal.kind) {
    case 'unknownOperation':
      return 'operationLog.rollback.refusalUnknown'
    case 'alreadyRollingBack':
      return 'operationLog.rollback.refusalAlreadyRollingBack'
    case 'alreadyRolledBack':
      return 'operationLog.rollback.refusalAlreadyRolledBack'
    case 'isARollback':
      return 'operationLog.rollback.refusalIsARollback'
    case 'notRollbackable':
      return notRollbackableNotice(refusal.detail)
    case 'volumeUnavailable':
      return 'operationLog.rollback.refusalVolumeUnavailable'
    case 'selection':
      return selectionRefusalNotice(refusal.detail)
  }
}

/**
 * Why the checked items couldn't be rolled back. The dialog keeps its button off at
 * "0 of M" and refuses a pick past the cap itself, so each of these is a race or a
 * backstop: the items changed under the tree, or it was still reading the operation.
 */
function selectionRefusalNotice(reason: SelectionRefusal): MessageKey {
  switch (reason) {
    case 'nothingSelected':
      return 'operationLog.rollback.refusalNothingSelected'
    case 'summaryNotBuilt':
      return 'operationLog.rollback.refusalSummaryNotBuilt'
    case 'tooManyToggles':
      return 'operationLog.rollback.refusalTooManyToggles'
  }
}

/**
 * Why THIS operation can't be reversed, one sentence per stored reason.
 *
 * A single "this can't be rolled back" would leave the user guessing whether they
 * did something wrong, and the answers differ in kind: a merge and a resolved name
 * clash lost the information a reversal would need, an overwrite and a permanent
 * delete kept no bytes to restore, an archive edit is a gap Cmdr hasn't closed yet,
 * and an incomplete record is Cmdr refusing to guess. None of them carry a next
 * step, so none of them pretend to.
 *
 * Shared by the refusal a press earns and the line a `notRollbackable` ROW carries
 * on sight. The row is the surface that matters: it never offers the button, so
 * without it the reason would have no way to reach the person reading the history.
 */
export function notRollbackableNotice(reason: NotRollbackableReason): MessageKey {
  switch (reason) {
    case 'overwrote':
      return 'operationLog.rollback.refusalOverwrote'
    case 'permanentDelete':
      return 'operationLog.rollback.refusalPermanentDelete'
    case 'archiveOverwrite':
      return 'operationLog.rollback.refusalArchiveOverwrite'
    case 'zipEditUnsupported':
      return 'operationLog.rollback.refusalZipEditUnsupported'
    case 'journalIncomplete':
      return 'operationLog.rollback.refusalJournalIncomplete'
    case 'directoryMerge':
      return 'operationLog.rollback.refusalDirectoryMerge'
    case 'stagedConflictResolved':
      return 'operationLog.rollback.refusalStagedConflictResolved'
  }
}
