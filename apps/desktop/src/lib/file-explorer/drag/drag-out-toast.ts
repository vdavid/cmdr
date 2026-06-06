import { composeTransferCompleteToast } from '$lib/file-operations/transfer/transfer-complete-toast'
import { pluralize } from '$lib/utils/pluralize'
import type { ToastLevel } from '$lib/ui/toast'

/**
 * The completion payload the backend emits once a drag-out SESSION drains
 * (`drag-out-session-complete`). Counts are TOP-LEVEL dragged items, split by
 * kind — consistent with the transfer toasts' selection-split contract.
 */
export interface DragOutSessionComplete {
  sessionKey: number
  /** Top-level files that landed successfully. */
  filesSucceeded: number
  /** Top-level folders that landed successfully. */
  foldersSucceeded: number
  /** Leaf names of items that failed (empty on full success). */
  failures: string[]
}

/** A composed toast: the message string plus the level that fits its meaning. */
export interface DragOutToast {
  message: string
  level: ToastLevel
}

/**
 * Composes the toast shown when a drag-out download session finishes.
 *
 * - **Full success** → the standard transfer-complete wording via the shared
 *   `composeTransferCompleteToast` ("Copied 2 files and 1 folder."), level
 *   `success`. Counts are the top-level dragged items the session downloaded.
 * - **Partial success** → the success phrase plus a tail naming what didn't make
 *   it ("…, but couldn't copy video.mov."), level `warn`. Finder shows its OWN
 *   NSError alert per failed item, so our toast complements rather than
 *   duplicates: it names the file(s) and stays quiet on the technical detail.
 * - **Total failure** → a failure-only line naming the file(s), level `error`.
 *   Still complements Finder's alert (which already explained the error).
 *
 * Mirrors the transfer-failure pattern: name the file, lean on Finder for the
 * gory error detail. The friendly NSError already rode the
 * `FriendlyError` pipeline on the backend.
 */
export function composeDragOutCompleteToast(payload: DragOutSessionComplete): DragOutToast {
  const { filesSucceeded, foldersSucceeded, failures } = payload
  const succeededCount = filesSucceeded + foldersSucceeded
  const failedCount = failures.length

  // Total failure: nothing landed.
  if (succeededCount === 0) {
    return { message: `Couldn't copy ${describeFailures(failures)}.`, level: 'error' }
  }

  // Build the success phrase through the shared composer (selection-split,
  // copy, no skips — folders always merge so skips don't apply to drag-out).
  const successPhrase = composeTransferCompleteToast({
    operationType: 'copy',
    filesProcessed: succeededCount,
    filesSkipped: 0,
    fileCount: filesSucceeded,
    folderCount: foldersSucceeded,
  })

  // Full success.
  if (failedCount === 0) {
    return { message: successPhrase, level: 'success' }
  }

  // Partial: success phrase + a tail naming the failures. Drop the trailing
  // period from the success phrase so the tail reads as one sentence.
  const successBody = successPhrase.replace(/\.$/, '')
  return {
    message: `${successBody}, but couldn't copy ${describeFailures(failures)}.`,
    level: 'warn',
  }
}

/**
 * Names the failed items: a single leaf reads in full ("video.mov"); two or
 * more collapse to a count ("3 files") so the toast doesn't grow unbounded on a
 * big multi-select that fails wholesale.
 */
function describeFailures(failures: string[]): string {
  if (failures.length === 1) return failures[0]
  return `${String(failures.length)} ${pluralize(failures.length, 'file')}`
}
