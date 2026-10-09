// Operation-manager IPC: the queue window's view of every running/queued
// copy, move, delete, and trash operation, plus the pause/resume/cancel
// controls. The thin `operations-changed` event carries membership + lifecycle
// status; live per-row bars come from the separate `write-progress` stream
// (see `onWriteProgress` in `write-operations.ts`).

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import type { OperationDetails, OperationSnapshot, OperationsChanged } from '$lib/ipc/bindings'

export type { OperationDetails, OperationSnapshot, OperationsChanged }

/** Snapshot of every operation the manager currently tracks (queued, running,
 *  paused, and recently-terminal until it's pruned). */
export async function listOperations(): Promise<OperationSnapshot[]> {
  return commands.listOperations()
}

/** The full source and destination paths of one operation and when it was
 *  registered and started, for the queue window's EXPANDED row. Fetched on
 *  demand, never carried on `operations-changed` (which stays thin).
 *
 *  Resolves `null` when the manager no longer tracks the operation (the typed
 *  `notFound` refusal): it settled between the row being drawn and the question
 *  arriving, so its row is about to leave too and there is nothing to report.
 *  Rejects only on a broken bridge, which the caller logs. */
export async function getOperationDetails(operationId: string): Promise<OperationDetails | null> {
  const res = await commands.getOperationDetails(operationId)
  if (res.status === 'ok') return res.data
  // `notFound` is the one refusal (`OperationDetailsError`), and it means the
  // operation already left. A new variant needs its own decision here.
  return null
}

/** Cancel one operation, keeping already-copied files (rollback = false). A
 *  queued op is dropped before it spawns; a running/paused op stops and keeps
 *  partials. */
export async function cancelOperation(operationId: string): Promise<void> {
  await commands.cancelOperation(operationId)
}

/** Cancel several operations at once (the "Cancel selected" action). Same
 *  keep-partials semantics as `cancelOperation`. */
export async function cancelOperations(operationIds: string[]): Promise<void> {
  await commands.cancelOperations(operationIds)
}

/** Pause one running operation in place. It keeps its lane slot and can be
 *  resumed; its snapshot status flips to `paused`.
 *
 *  The command answers with a `PauseOutcome`, dropped here on purpose: every
 *  frontend surface renders the live status from `operations-changed`, so none
 *  of them has to trust a return value. The MCP `queue` tool is the consumer
 *  that reads it, since an agent has nothing else to go on. */
export async function pauseOperation(operationId: string): Promise<void> {
  await commands.pauseOperation(operationId)
}

/** Resume one paused operation. Its `PauseOutcome` is dropped for the same
 *  reason as `pauseOperation`'s. */
export async function resumeOperation(operationId: string): Promise<void> {
  await commands.resumeOperation(operationId)
}

/** Pause every running operation. Its `PauseAllOutcome` counts are dropped for
 *  the same reason `pauseOperation`'s single outcome is: the queue window renders
 *  the live statuses. The MCP `queue` tool reads them. */
export async function pauseAll(): Promise<void> {
  await commands.pauseAll()
}

/** Resume every paused operation. Its counts are dropped like `pauseAll`'s. */
export async function resumeAll(): Promise<void> {
  await commands.resumeAll()
}

/** Drop one retained failure from the snapshot. Dismissal is always explicit —
 *  a failed row waits until someone reads it, however long that takes. */
export async function dismissFailedOperation(operationId: string): Promise<void> {
  await commands.dismissFailedOperation(operationId)
}

/** Drop every retained failure (the queue toolbar's "Dismiss all"). */
export async function dismissAllFailedOperations(): Promise<void> {
  await commands.dismissAllFailedOperations()
}

/** Subscribe to the thin registry snapshot (membership + lifecycle status). The
 *  queue window reduces this into its row set. Returns an `UnlistenFn`; call it
 *  on teardown or you leak the listener. */
export async function onOperationsChanged(callback: (event: OperationsChanged) => void): Promise<UnlistenFn> {
  return events.operationsChanged.listen((event) => {
    callback(event.payload)
  })
}
