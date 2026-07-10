// Operation-log read side: the recent-operations feed and one operation's detail.
// Thin wrappers over the typed `commands.*` bindings, unwrapping `Result<T, string>`.
// The alpha dialog and any future surface consume these; the Debug panel reads the
// same backend commands directly (dev-only, bindings-import-exempt).

import { commands, type OperationRow, type OperationItemView } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { OperationRow, OperationItemView }

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
