/**
 * Client-side labels for the operation-log dialog.
 *
 * Every label is derived from a TYPED enum field, never a display string the
 * backend rendered (Finding 3 / `no-string-matching`): the per-operation summary
 * ("Moved 214 items") is formatted here from `kind` + `itemCount` via an ICU
 * plural key, so it localizes per viewer and shows a thousands separator. Status,
 * kind, initiator, and item-outcome labels map their enum to a catalog key with
 * an exhaustive switch (a new variant is a compile error until it's mapped).
 */

import type { Initiator, ItemOutcome, OpKind, ArchiveSubkind, ExecutionStatus, RollbackState } from '$lib/ipc/bindings'
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

/** Who started the operation: you, an external AI client (over MCP), or the agent. */
export function initiatorLabel(initiator: Initiator): string {
  switch (initiator) {
    case 'user':
      return tString('operationLog.initiator.user')
    case 'aiClient':
      return tString('operationLog.initiator.aiClient')
    case 'agent':
      return tString('operationLog.initiator.agent')
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
