import { composeTransferCompleteToast } from '$lib/file-operations/transfer/transfer-complete-toast'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
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
 * - **Partial success** → the success sentence plus a second one naming what
 *   didn't make it ("Copied 2 files. Couldn’t copy “video.mov”."), level `warn`. Finder shows its OWN
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
    return { message: tString('fileExplorer.dragOut.failed', failureParams(failures)), level: 'error' }
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

  // Partial: the success sentence whole, then one naming the failures. Two
  // sentences, so no language has to splice a clause onto a finished one.
  return {
    message: tString('fileExplorer.dragOut.partial', { summary: successPhrase, ...failureParams(failures) }),
    level: 'warn',
  }
}

/**
 * The catalog params that name the failed items: a single leaf reads in full
 * ("video.mov"); two or more collapse to a count ("3 items") so the toast
 * doesn't grow unbounded on a big multi-select that fails wholesale. The
 * `=1` branch in the catalog picks the name.
 */
function failureParams(failures: string[]): { count: number; countText: string; name: string } {
  return { count: failures.length, countText: formatInteger(failures.length), name: failures[0] ?? '' }
}
