import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { pluralize } from '$lib/utils/pluralize'
import type { TransferOperationType } from '$lib/file-explorer/types'

export interface TransferCompleteToastInput {
  operationType: TransferOperationType
  /** Every source file the operation considered (transferred + skipped). Mirrors the BE's `filesProcessed`. */
  filesProcessed: number
  /** Subset of `filesProcessed` that the user (or upfront policy) chose to skip via conflict resolution. */
  filesSkipped: number
  /** Top-level files the user SELECTED (not interior counts). Omitted on the clipboard-paste path,
   *  where the selection's per-type split isn't known. */
  fileCount?: number
  /** Top-level folders the user SELECTED. Folders always merge, so they're never skipped. */
  folderCount?: number
}

/**
 * Composes the toast shown when a copy/move/trash/delete operation completes.
 *
 * Copy and move report what the user SELECTED at the top level, split by type:
 * "Moved 1 file and 3 folders". Interior counts never surface — moving one folder
 * with thousands of files inside still reads as one folder. When the user skipped
 * clashing files, the skipped count appears as a suffix (skips are always file-level
 * because folders always merge).
 *
 * The clipboard-paste path has no per-type selection split, so it falls back to the
 * file-count wording ("Copy complete: copied 5 files.").
 *
 * Trash and delete don't have a skip or merge concept and keep the historic short wording.
 */
export function composeTransferCompleteToast(input: TransferCompleteToastInput): string {
  const { operationType, filesProcessed, filesSkipped, fileCount, folderCount } = input

  if (operationType === 'trash') {
    return `Moved ${formatNumber(filesProcessed)} ${pluralize(filesProcessed, 'file')} to trash`
  }
  if (operationType === 'delete') {
    return `Delete complete: ${formatNumber(filesProcessed)} ${pluralize(filesProcessed, 'file')}`
  }

  const verbPast = operationType === 'copy' ? 'Copied' : 'Moved'

  // Selection split: report the top-level items the user picked. Folders always
  // merge (never skipped), so the moved-file count is `fileCount - filesSkipped`.
  if (fileCount !== undefined && folderCount !== undefined) {
    const movedFiles = fileCount - filesSkipped
    const movedPhrase = describeCounts(movedFiles, folderCount)

    if (movedPhrase === null) {
      // Nothing actually landed (no folders, every selected file skipped).
      // Fall through to the file-only all-skipped wording below.
      return composeFileOnlyToast(operationType, filesProcessed, filesSkipped)
    }

    if (filesSkipped === 0) {
      return `${verbPast} ${movedPhrase}.`
    }
    return `${verbPast} ${movedPhrase}, skipped ${formatNumber(filesSkipped)} ${pluralize(filesSkipped, 'file')} (already at the target).`
  }

  // Fallback (clipboard paste): no selection split available, report file counts.
  return composeFileOnlyToast(operationType, filesProcessed, filesSkipped)
}

/**
 * Builds the "N files and M folders" phrase, omitting any zero part. Returns
 * `null` when both counts are zero (nothing to report).
 */
function describeCounts(files: number, folders: number): string | null {
  const parts: string[] = []
  if (files > 0) parts.push(`${formatNumber(files)} ${pluralize(files, 'file')}`)
  if (folders > 0) parts.push(`${formatNumber(folders)} ${pluralize(folders, 'folder')}`)
  if (parts.length === 0) return null
  return parts.join(' and ')
}

/** The file-count-only wording used by the clipboard-paste path and the all-skipped collapse. */
function composeFileOnlyToast(
  operationType: TransferOperationType,
  filesProcessed: number,
  filesSkipped: number,
): string {
  const transferred = filesProcessed - filesSkipped
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
