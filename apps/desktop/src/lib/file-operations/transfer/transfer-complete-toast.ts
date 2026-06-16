/**
 * Composes the toast shown when a copy/move/trash/delete operation completes.
 *
 * This is the i18n pilot (the hardest existing multi-variable case): every
 * wording lives in `transfer.*` catalog keys resolved through `t()` and ICU
 * `select` + `plural`, NOT hardcoded English. The branch SHAPE matches the
 * historic composer exactly; `transfer-complete-toast.test.ts` is the parity
 * net asserting the rendered en output is byte-identical to the pre-i18n copy.
 *
 * Copy and move report what the user SELECTED at the top level, split by type:
 * "Moved 1 file and 3 folders". Interior counts never surface — moving one
 * folder with thousands of files inside still reads as one folder. When the user
 * skipped clashing files, the skipped count appears as a suffix (skips are
 * always file-level because folders always merge). When the per-type split is
 * unknown (a top-level kind probe came back partial), it falls back to the
 * file-count wording. Trash and delete keep the historic short wording.
 *
 * Two restructurings ICU needs (raw counts alone can't express the wording):
 *  - The "N files and M folders" join omits a zero part. ICU plural branches are
 *    independent and can't see each other's emptiness, so the caller passes a
 *    `kind` discriminator (`both` | `filesOnly` | `foldersOnly`) and the message
 *    `select`s on it.
 *  - Counts are embedded as `$lib/intl`-preformatted STRINGS (`formatNumber`)
 *    passed as `*Text` params, keeping number formatting single-sourced in
 *    `$lib/intl`. The raw integer is passed alongside ONLY to drive ICU plural
 *    selection (noun + was/were), never for display.
 */

import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { tString } from '$lib/intl/messages.svelte'
import type { TransferOperationType } from '$lib/file-explorer/types'

export interface TransferCompleteToastInput {
  operationType: TransferOperationType
  /** Every source file the operation considered (transferred + skipped). Mirrors the BE's `filesProcessed`. */
  filesProcessed: number
  /** Subset of `filesProcessed` that the user (or upfront policy) chose to skip via conflict resolution. */
  filesSkipped: number
  /** Top-level files the operation transferred (not interior counts). Supplied by F5/F6,
   *  drag-and-drop, and clipboard paste; omitted only when a top-level kind probe came
   *  back partial, where the composer falls back to the flattened file-count wording. */
  fileCount?: number
  /** Top-level folders the operation transferred. Folders always merge, so they're never skipped. */
  folderCount?: number
}

/** Composes the copy/move/trash/delete completion toast from catalog keys. */
export function composeTransferCompleteToast(input: TransferCompleteToastInput): string {
  const { operationType, filesProcessed, filesSkipped, fileCount, folderCount } = input

  if (operationType === 'trash') {
    return tString('transfer.trash', { countText: formatNumber(filesProcessed), count: filesProcessed })
  }
  if (operationType === 'delete') {
    return tString('transfer.delete', { countText: formatNumber(filesProcessed), count: filesProcessed })
  }

  const verb = verbParam(operationType)

  // Selection split: report the top-level items the user picked. Folders always
  // merge (never skipped), so the moved-file count is `fileCount - filesSkipped`.
  if (fileCount !== undefined && folderCount !== undefined) {
    const movedFiles = fileCount - filesSkipped
    const phrase = describeCounts(movedFiles, folderCount)

    if (phrase === null) {
      // Nothing actually landed (no folders, every selected file skipped).
      return composeFileOnlyToast(operationType, filesProcessed, filesSkipped)
    }

    if (filesSkipped === 0) {
      return tString('transfer.split.clean', { verb, phrase })
    }
    return tString('transfer.split.skipped', {
      verb,
      phrase,
      skippedText: formatNumber(filesSkipped),
      skipped: filesSkipped,
    })
  }

  // Fallback (clipboard paste): no selection split available, report file counts.
  return composeFileOnlyToast(operationType, filesProcessed, filesSkipped)
}

/** `'copy'` stays `'copy'`; everything else maps to the `other` (move) ICU branch. */
function verbParam(operationType: TransferOperationType): string {
  return operationType === 'copy' ? 'copy' : 'move'
}

/**
 * Builds the "N files and M folders" phrase via the `kind`-discriminated ICU
 * message, omitting any zero part. Returns `null` when both counts are zero.
 */
function describeCounts(files: number, folders: number): string | null {
  if (files <= 0 && folders <= 0) return null
  const kind = files > 0 && folders > 0 ? 'both' : folders > 0 ? 'foldersOnly' : 'filesOnly'
  return tString('transfer.movedPhrase', {
    kind,
    filesText: formatNumber(files),
    files,
    foldersText: formatNumber(folders),
    folders,
  })
}

/** The file-count-only wording (clipboard paste + the all-skipped collapse). */
function composeFileOnlyToast(
  operationType: TransferOperationType,
  filesProcessed: number,
  filesSkipped: number,
): string {
  const transferred = filesProcessed - filesSkipped
  const verb = verbParam(operationType)

  // All transferred, none skipped.
  if (filesSkipped === 0) {
    return tString('transfer.fileOnly.allDone', {
      verb,
      transferredText: formatNumber(transferred),
      transferred,
    })
  }

  // Nothing transferred, all skipped.
  if (transferred === 0) {
    if (filesSkipped === 1) {
      return tString('transfer.fileOnly.allSkippedSingle', { verb })
    }
    return tString('transfer.fileOnly.allSkippedMany', { verb, skippedText: formatNumber(filesSkipped) })
  }

  // Mixed: some transferred, some skipped.
  if (operationType === 'copy') {
    return tString('transfer.fileOnly.mixedCopy', {
      transferredText: formatNumber(transferred),
      skippedText: formatNumber(filesSkipped),
      processedText: formatNumber(filesProcessed),
      processed: filesProcessed,
    })
  }
  return tString('transfer.fileOnly.mixedMove', {
    transferredText: formatNumber(transferred),
    skippedText: formatNumber(filesSkipped),
    skipped: filesSkipped,
  })
}
