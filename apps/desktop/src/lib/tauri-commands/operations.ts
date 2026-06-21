// Operation-manager IPC: the queue window's view of every running/queued
// copy, move, delete, and trash operation, plus the pause/resume/cancel
// controls. The thin `operations-changed` event carries membership + lifecycle
// status; live per-row bars come from the separate `write-progress` stream
// (see `onWriteProgress` in `write-operations.ts`).

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import type { OperationSnapshot, OperationsChanged } from '$lib/ipc/bindings'

export type { OperationSnapshot, OperationsChanged }

/** Snapshot of every operation the manager currently tracks (queued, running,
 *  paused, and recently-terminal until it's pruned). */
export async function listOperations(): Promise<OperationSnapshot[]> {
  return commands.listOperations()
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
 *  resumed; its snapshot status flips to `paused`. */
export async function pauseOperation(operationId: string): Promise<void> {
  await commands.pauseOperation(operationId)
}

/** Resume one paused operation. */
export async function resumeOperation(operationId: string): Promise<void> {
  await commands.resumeOperation(operationId)
}

/** Pause every running operation. */
export async function pauseAll(): Promise<void> {
  await commands.pauseAll()
}

/** Resume every paused operation. */
export async function resumeAll(): Promise<void> {
  await commands.resumeAll()
}

/** Subscribe to the thin registry snapshot (membership + lifecycle status). The
 *  queue window reduces this into its row set. Returns an `UnlistenFn`; call it
 *  on teardown or you leak the listener. */
export async function onOperationsChanged(callback: (event: OperationsChanged) => void): Promise<UnlistenFn> {
  return events.operationsChanged.listen((event) => {
    callback(event.payload)
  })
}
