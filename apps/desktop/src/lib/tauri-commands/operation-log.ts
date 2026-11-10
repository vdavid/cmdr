// Operation log: the recent-operations feed, one operation's detail, its item tree, and undo.
// Thin wrappers over the typed `commands.*` bindings, unwrapping `Result<T, string>`.
// The alpha dialog and any future surface consume these; the Debug panel reads the
// same backend commands directly (dev-only, bindings-import-exempt).

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type Initiator,
  type OperationRow,
  type OperationItemView,
  type OperationTreeRequest,
  type OperationTreeView,
  type OperationUndoOutcome,
  type RollbackDispatch,
  type RollbackSelection,
  type SkipBreakdown,
  type SkipReason,
  type TreeBackfillEvent,
  type TreeBackfillStart,
  type UndoReport,
} from '#lib/ipc/bindings.js'
import { throwRollbackRefusal } from '#lib/operation-log/rollback-refusal.js'
import { throwIpcError } from './ipc-types'

export type {
  OperationRow,
  OperationItemView,
  OperationTreeRequest,
  OperationTreeView,
  OperationUndoOutcome,
  RollbackDispatch,
  SkipBreakdown,
  SkipReason,
  TreeBackfillEvent,
  TreeBackfillStart,
  UndoReport,
}

/**
 * One window of an operation's item tree: the rows under the request's expanded
 * folders from its offset, with the selection normalized and counted by the backend.
 * `null` when the operation is absent (pruned by retention since the list was read).
 */
export async function getOperationLogTree(
  operationId: string,
  request: OperationTreeRequest,
): Promise<OperationTreeView | null> {
  const res = await commands.getOperationLogTree(operationId, request)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Ask for an operation's tree summary: start the backfill of an operation journaled
 * before the summary existed, join the one running, or learn why none is needed or
 * possible. Returns at once; a started or joined backfill reports each step through
 * {@link onOperationLogTreeBackfill}.
 */
export async function startOperationLogTreeBackfill(operationId: string): Promise<TreeBackfillStart> {
  const res = await commands.startOperationLogTreeBackfill(operationId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Ask an operation's running tree backfill to stop (the dialog closed, or moved to another operation). */
export function cancelOperationLogTreeBackfill(operationId: string): Promise<void> {
  return commands.cancelOperationLogTreeBackfill(operationId)
}

/** Every step of every tree backfill (progress, then one `finished`), for whichever operation it names. */
export function onOperationLogTreeBackfill(handler: (event: TreeBackfillEvent) => void): Promise<UnlistenFn> {
  return events.operationLogTreeBackfill.listen((event) => {
    handler(event.payload)
  })
}

/**
 * The writer's build of the journal's folder indexes settled, built or not, so a tree that
 * answered `indexesBuilding` reads again and finds its rows, or `indexesUnavailable`.
 * Fires once per launch (the first launch after the upgrade that added them is the only
 * one that waits); a listener that comes later finds it settled.
 */
export function onOperationLogTreeIndexesSettled(handler: () => void): Promise<UnlistenFn> {
  return events.operationLogTreeIndexesSettled.listen(() => {
    handler()
  })
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
 *
 * `selection` narrows it to what the item tree has checked: send back the normalized
 * selection the tree last answered with. `null` (the default) reverses everything
 * that still awaits it, which is also "Roll back the rest".
 */
export async function rollbackOperation(
  operationId: string,
  initiator: Initiator,
  selection: RollbackSelection | null = null,
): Promise<RollbackDispatch> {
  const res = await commands.rollbackOperation(operationId, initiator, selection)
  if (res.status === 'error') throwRollbackRefusal(res.error)
  return res.data
}
