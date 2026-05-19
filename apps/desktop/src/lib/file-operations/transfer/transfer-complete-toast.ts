import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { pluralize } from '$lib/utils/pluralize'
import type { TransferOperationType } from '$lib/file-explorer/types'

export interface TransferCompleteToastInput {
  operationType: TransferOperationType
  /** Every source the operation considered (transferred + skipped). Mirrors the BE's `filesProcessed`. */
  filesProcessed: number
  /** Subset of `filesProcessed` that the user (or upfront policy) chose to skip via conflict resolution. */
  filesSkipped: number
}

/**
 * Composes the toast shown when a copy/move/trash/delete operation completes.
 *
 * The user-relevant cases are copy and move with skipped files: `"Copy complete: 0 files"`
 * is misleading when the user picked "Skip" upfront and the system honored it. We surface
 * `copied` vs `skipped` separately and, for the mixed case, summarize what's at the target
 * so the report reflects the actual outcome.
 *
 * Trash and delete don't have a skip concept and keep the historic short wording.
 */
export function composeTransferCompleteToast(input: TransferCompleteToastInput): string {
  const { operationType, filesProcessed, filesSkipped } = input
  const transferred = filesProcessed - filesSkipped

  if (operationType === 'trash') {
    return `Moved ${formatNumber(filesProcessed)} ${pluralize(filesProcessed, 'file')} to trash`
  }
  if (operationType === 'delete') {
    return `Delete complete: ${formatNumber(filesProcessed)} ${pluralize(filesProcessed, 'file')}`
  }

  const verbPast = operationType === 'copy' ? 'copied' : 'moved'
  const verbNoun = operationType === 'copy' ? 'Copy' : 'Move'

  // All transferred, none skipped.
  if (filesSkipped === 0) {
    return `${verbNoun} complete: ${verbPast} ${formatNumber(transferred)} ${pluralize(transferred, 'file')}.`
  }

  // Nothing transferred, all skipped.
  if (transferred === 0) {
    if (filesSkipped === 1) {
      return `${verbNoun} complete: file already at the target, not ${verbPast}.`
    }
    return `${verbNoun} complete: skipped all ${formatNumber(filesSkipped)} files (already at the target), nothing was ${verbPast}.`
  }

  // Mixed: some transferred, some skipped.
  const tail =
    operationType === 'copy'
      ? `All ${formatNumber(filesProcessed)} of your selected ${pluralize(filesProcessed, 'file')} are now at the target.`
      : `${formatNumber(filesSkipped)} ${pluralize(filesSkipped, 'file')} ${filesSkipped === 1 ? 'was' : 'were'} already at the target.`
  return `${verbNoun} complete: ${verbPast} ${formatNumber(transferred)}, skipped ${formatNumber(filesSkipped)}. ${tail}`
}
