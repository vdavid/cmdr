/**
 * The press behind whichever rollback question is on screen, so `dialog_confirm` can answer it
 * however it got raised. `RollbackConfirmDialog` registers itself: every host goes through it,
 * so a new host can't be missed. Per window, so the queue window's question isn't reachable from
 * the main window's `mcp-confirm-dialog`, and that answers "not ready".
 */

import type { RollbackRefusal } from '#lib/ipc/bindings.js'

/** What a rollback question's yes led to, on the wire `dialog_confirm` answers with. */
export type RollbackAnswer =
  /** `inverseOpId` is `null` for a running operation, which stops in place. */
  | { status: 'dispatched'; inverseOpId: string | null }
  /** The question came down either way. `alreadyStopping`: a stop of this operation is already on its way. */
  | { status: 'refused'; refusal: RollbackRefusal | { kind: 'alreadyStopping' } }

/** What a host's yes returns: the answer, or nothing for a running operation stopped in place. */
// eslint-disable-next-line @typescript-eslint/no-invalid-void-type -- a host that stops a running operation has nothing to say, and its plain callback fits
export type RollbackConfirmResult = RollbackAnswer | Promise<RollbackAnswer> | void

const STOPPING: RollbackAnswer = { status: 'dispatched', inverseOpId: null }

/** Mounted questions, oldest first: the last is the one on top. */
const confirmers: (() => RollbackConfirmResult)[] = []

/** Registers a mounted question's yes. Returns the unregister, for its unmount. */
export function registerRollbackConfirmer(confirm: () => RollbackConfirmResult): () => void {
  confirmers.push(confirm)
  return () => {
    const index = confirmers.indexOf(confirm)
    if (index !== -1) confirmers.splice(index, 1)
  }
}

/** Presses the top question's yes and answers what it led to, or `null` with none mounted. */
export async function pressRollbackConfirm(): Promise<RollbackAnswer | null> {
  const confirm = confirmers.at(-1)
  if (confirm === undefined) return null
  return (await confirm()) ?? STOPPING
}
