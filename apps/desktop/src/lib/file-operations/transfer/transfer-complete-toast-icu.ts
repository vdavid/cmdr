/**
 * SPIKE (Milestone 0): the ICU-backed twin of `composeTransferCompleteToast`,
 * proving the i18n runtime + `intl-messageformat` can express every branch of
 * the hardest existing toast at en parity. The branch SHAPE matches the
 * original exactly; only the wording moves into `transfer.*` catalog keys
 * resolved through `t()`.
 *
 * Two restructurings the plan calls for (raw counts alone can't express the
 * wording):
 *  - The "N files and M folders" join omits a zero part. ICU plural branches
 *    are independent and can't see each other's emptiness, so the caller passes
 *    a `kind` discriminator (`both` | `filesOnly` | `foldersOnly`) and the
 *    message `select`s on it.
 *  - Counts are embedded as `$lib/intl`-preformatted STRINGS (`formatNumber`)
 *    passed as `*Text` params, keeping number formatting single-sourced in
 *    `$lib/intl`. The raw integer is passed alongside ONLY to drive ICU plural
 *    selection (noun + was/were), never for display.
 *
 * Not wired into the app: M2 finalizes the migration. This file exists so the
 * parity test can assert the ICU twin equals the live composer over the full
 * matrix.
 */

import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { tString } from '$lib/intl/messages.svelte'
import type { TransferOperationType } from '$lib/file-explorer/types'
import type { TransferCompleteToastInput } from './transfer-complete-toast'

/** ICU twin of `composeTransferCompleteToast`. Same branches, catalog wording. */
export function composeTransferCompleteToastIcu(input: TransferCompleteToastInput): string {
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
    const phrase = describeCountsIcu(movedFiles, folderCount)

    if (phrase === null) {
      // Nothing actually landed (no folders, every selected file skipped).
      return composeFileOnlyToastIcu(operationType, filesProcessed, filesSkipped)
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
  return composeFileOnlyToastIcu(operationType, filesProcessed, filesSkipped)
}

/** `'copy'` stays `'copy'`; everything else maps to the `other` (move) ICU branch. */
function verbParam(operationType: TransferOperationType): string {
  return operationType === 'copy' ? 'copy' : 'move'
}

/**
 * Builds the "N files and M folders" phrase via the `kind`-discriminated ICU
 * message, omitting any zero part. Returns `null` when both counts are zero.
 */
function describeCountsIcu(files: number, folders: number): string | null {
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
function composeFileOnlyToastIcu(
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
