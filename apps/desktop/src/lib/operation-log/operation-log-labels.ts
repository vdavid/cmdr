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
  OpKind,
  ArchiveSubkind,
  ExecutionStatus,
  RollbackRefusal,
  RollbackState,
} from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { RollbackConfirmVariant } from '$lib/file-operations/rollback-confirm-variant'
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
 * Which confirmation to put in front of Roll back, because what a rollback DOES
 * depends on what was done: undoing a copy deletes, undoing a move carries the files
 * home, undoing a rename only changes names back.
 *
 * Mirrors the backend's `inverse_kind` (`operation_log/rollback.rs`) arm for arm,
 * including its `delete → delete` arm: a permanent delete is never rollbackable, so
 * the button never appears on one, and the arm exists so a NEW `OpKind` is a compile
 * error here rather than a confidently wrong sentence in front of a user.
 */
export function rollbackConfirmVariant(kind: OpKind): RollbackConfirmVariant {
  switch (kind) {
    case 'copy':
    case 'createFolder':
    case 'createFile':
    case 'archiveEdit':
    case 'delete':
      return 'undoByDeleting'
    case 'move':
    case 'trash':
      return 'undoByMovingBack'
    case 'rename':
      return 'undoByRenamingBack'
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
      return 'operationLog.rollback.refusalNotRollbackable'
    case 'volumeUnavailable':
      return 'operationLog.rollback.refusalVolumeUnavailable'
  }
}
