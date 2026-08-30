/**
 * Client-side labels for the operation-log dialog, plus the two other things the
 * dialog derives from a typed enum: which rollback confirmation to raise, and which
 * sentence to show when a rollback is refused.
 *
 * Every one of them comes from a TYPED enum field, never a display string the
 * backend rendered: the per-operation summary
 * ("Moved 214 items") is formatted here from `kind` + `itemCount` via an ICU
 * plural key, so it localizes per viewer and shows a thousands separator. Status,
 * kind, initiator, and item-outcome labels map their enum to a catalog key with
 * an exhaustive switch (a new variant is a compile error until it's mapped).
 */

import type {
  Initiator,
  ItemOutcome,
  NotRollbackableReason,
  OpKind,
  ArchiveSubkind,
  ExecutionStatus,
  RollbackRefusal,
  RollbackState,
} from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'

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

/**
 * The one-line summary of an operation ("Moved 214 items"), formatted from the
 * typed `kind` (+ archive subkind) and `itemCount`. The plural form and the
 * thousands separator both follow the active locale.
 */
export function operationSummary(kind: OpKind, subkind: ArchiveSubkind | null, itemCount: number): string {
  return tString(summaryKey(kind, subkind), { count: itemCount, countText: formatInteger(itemCount) })
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
      return tString('operationLog.status.done')
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
 * `finish` is a `partiallyRolledBack` row picking a reversal back up. The engine
 * admits that state through the same gate as `rollbackable`
 * (`operation_log/rollback.rs`, `check_rollbackable`) and re-attempts every item
 * with a fresh recheck, so an item the first pass already reversed reads as gone
 * and is credited without acting. ❌ Never widen this to a state the backend gate
 * refuses: the row would offer a press that can only come back as a refusal.
 */
export type RowRollbackAction = 'start' | 'finish'

export function rowRollbackAction(state: RollbackState): RowRollbackAction | null {
  switch (state) {
    case 'rollbackable':
      return 'start'
    case 'partiallyRolledBack':
      return 'finish'
    // Nothing to reverse, or a reversal is already running.
    case 'notRollbackable':
    case 'rollingBack':
    case 'rolledBack':
      return null
  }
}

/**
 * The button's words, which name what THIS press does: starting a reversal and
 * picking a stopped one back up are different promises, and a row that offers
 * "Roll back" on an operation already half reversed would be making the wrong one.
 */
export function rowRollbackActionLabel(action: RowRollbackAction): string {
  switch (action) {
    case 'start':
      return tString('operationLog.dialog.rollBack')
    case 'finish':
      return tString('operationLog.dialog.finishRollBack')
  }
}

/**
 * The sentence a row carries ON SIGHT, with no press to earn it, or `null` when the
 * badge says everything there is to say.
 *
 * Two states need one. A `notRollbackable` row never offers the button whose refusal
 * would otherwise carry its reason, so the reason has no other way to reach the
 * reader. A `partiallyRolledBack` row DOES offer a button, but its badge alone leaves
 * a person who cancelled a reversal unable to tell what became of their files.
 *
 * Exhaustive over `RollbackState`, so a new state has to decide whether it explains
 * itself rather than falling silent by default.
 */
export function rowStandingNotice(state: RollbackState, reason: NotRollbackableReason | null): MessageKey | null {
  switch (state) {
    case 'notRollbackable':
      // A NULL reason is an operation still running, which opens `not_rollbackable`
      // until finalize decides. A dangling label would be worse than silence.
      return reason === null ? null : notRollbackableNotice(reason)
    case 'partiallyRolledBack':
      return 'operationLog.rollback.partiallyRolledBackNotice'
    case 'rollbackable':
    case 'rollingBack':
    case 'rolledBack':
      return null
  }
}

/** A per-item outcome shown in the expanded item list. */
export function itemOutcomeLabel(outcome: ItemOutcome): string {
  switch (outcome) {
    case 'done':
      return tString('operationLog.outcome.done')
    case 'skipped':
      return tString('operationLog.outcome.skipped')
    case 'failed':
      return tString('operationLog.outcome.failed')
    case 'rolledBack':
      return tString('operationLog.outcome.rolledBack')
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
    case 'notRollbackable':
      return notRollbackableNotice(refusal.detail)
    case 'volumeUnavailable':
      return 'operationLog.rollback.refusalVolumeUnavailable'
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
