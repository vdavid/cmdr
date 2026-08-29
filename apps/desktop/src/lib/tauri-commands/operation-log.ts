// Operation log: the recent-operations feed, one operation's detail, and undo.
// Thin wrappers over the typed `commands.*` bindings, unwrapping `Result<T, string>`.
// The alpha dialog and any future surface consume these; the Debug panel reads the
// same backend commands directly (dev-only, bindings-import-exempt).

import {
  commands,
  type OperationRow,
  type OperationItemView,
  type OperationUndoOutcome,
  type RollbackDispatch,
  type SkipBreakdown,
  type SkipReason,
  type UndoReport,
} from '$lib/ipc/bindings'
import { throwRollbackRefusal } from '$lib/operation-log/rollback-refusal'
import { throwIpcError } from './ipc-types'

export type {
  OperationRow,
  OperationItemView,
  OperationUndoOutcome,
  RollbackDispatch,
  SkipBreakdown,
  SkipReason,
  UndoReport,
}

/** One operation's header plus a page of its items, dir prefixes resolved to full paths. */
export interface OperationLogDetail {
  operation: OperationRow
  items: OperationItemView[]
  /** Total item count across every `rowRole`, so the caller knows if more items exist. */
  totalItems: number
}

/**
 * The recent-operations feed (newest first), paged: the dialog's "last 50 + load 50
 * more". A missing/unopened journal yields an empty list rather than throwing.
 */
export async function getRecentOperationLogEntries(limit: number, offset: number): Promise<OperationRow[]> {
  const res = await commands.getRecentOperationLogEntries(limit, offset)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * One operation's header plus a page of its items. `null` when the operation is
 * absent (for example, pruned by retention between listing and expansion).
 */
export async function getOperationLogDetail(
  operationId: string,
  itemLimit: number,
  itemOffset: number,
): Promise<OperationLogDetail | null> {
  const res = await commands.getOperationLogDetail(operationId, itemLimit, itemOffset)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Undo these operations as one action. **Pass the ids in the order they were
 * APPLIED**: the backend reverses them newest first (a later batch can have taken a
 * name an earlier one freed), and the apply order is what breaks a same-second tie.
 *
 * Resolves only once every operation has been reversed, with the full tally — so
 * there's no polling here. It can take a while: each inverse is a queued operation
 * and waits out anything already working the same volume.
 */
export async function undoOperations(operationIds: string[]): Promise<UndoReport> {
  const res = await commands.undoOperations(operationIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Roll ONE operation back, and hand the reversal to the operation queue.
 *
 * Resolves as soon as the inverse is queued, not when it finishes: from that moment
 * the reversal is a normal managed operation, so the status corner and the queue
 * window own it. An `Ok` also means the operation is already recorded as rolling
 * back, so a caller can flip its own row without re-reading the journal.
 *
 * A refusal arrives TYPED (`asRollbackRefusal`), because each reason gets its own
 * sentence: already rolling back, already rolled back, a drive that went away.
 */
export async function rollbackOperation(operationId: string): Promise<RollbackDispatch> {
  const res = await commands.rollbackOperation(operationId)
  if (res.status === 'error') throwRollbackRefusal(res.error)
  return res.data
}
