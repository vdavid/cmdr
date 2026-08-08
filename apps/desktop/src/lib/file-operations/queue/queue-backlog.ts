/**
 * Is there anything in the queue besides the operation in front of you?
 *
 * The progress dialog's background button asks this to pick its word: with an
 * empty queue you're not queueing behind anything, you're backgrounding. A pure
 * function over the store's rows, like `status-corner/operation-chip.ts`, so
 * every gate is provable without a DOM.
 */

import { isInstantOperation, isTerminalStatus, type OperationRow } from './operations-store.svelte'

/**
 * Whether the queue holds live work other than `selfOperationId`.
 *
 * Three gates, each one a wrong word if it's missing:
 *  - the caller's OWN operation is skipped. It's in the queue for as long as the
 *    dialog is up, so counting it would pin the label to "Queue" forever.
 *  - instant ops (rename, create folder/file) never count: they're over before
 *    the eye lands on them, and a word that flickers is worse than either word.
 *  - only live work counts (`queued` / `running` / `paused`). A retained failure
 *    is a notice, not something you'd wait behind, and a `done` / `cancelled`
 *    row is on its way out of the list.
 *
 * `selfOperationId` is nullable because the dialog only learns its id once the
 * start command answers. Nothing to exclude then, which is also nothing to hide:
 * the button doesn't render until the id lands (`canPauseOrQueue`).
 */
export function hasOtherQueuedWork(rows: OperationRow[], selfOperationId: string | null): boolean {
  return rows.some(
    (row) =>
      row.snapshot.operationId !== selfOperationId &&
      !isInstantOperation(row.snapshot.operationType) &&
      !isTerminalStatus(row.snapshot.status),
  )
}
