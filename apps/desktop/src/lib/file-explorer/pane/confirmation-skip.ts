/**
 * The "Skip confirmation" setting (`fileOperations.skipConfirmation`): which
 * copy, move, and trash confirmations it may leave out, and the answer a skipped
 * one gives in the dialog's place.
 *
 * Every entry point (F5/F6/F8, the F-key bar, the palette, the menus, drag and
 * drop) opens its confirmation through `dialog-state.svelte.ts`'s `showTransfer`
 * / `showDeleteConfirmation`, and those two are the only callers here. So the
 * skip is decided once, after each entry point's own guards have already run
 * (read-only destination, search results, operation gate).
 *
 * A skipped dialog answers with exactly what it would have preselected: the
 * destination it prefills (the other pane, volume-relative like its path box),
 * the volume it opened on, "Ask for each" on conflicts (the progress dialog asks),
 * and no scan preview (the backend walks the sources itself, as it does for
 * paste and Duplicate).
 *
 * The dialog still shows whenever it carries something the user has to see or
 * decide, which is what every `null` below means:
 *
 * - A permanent delete, ever. That covers Shift+F8, a volume with no trash, an
 *   archive entry, and online-only cloud content (all arrive as `isPermanent` or
 *   `!supportsTrash`), plus a cloud FOLDER whose scan may turn the trash into a
 *   delete mid-walk.
 * - Compress (it names the new archive) and rename mode (the S3 rename that
 *   copies; a billed operation the user opted into via F2).
 * - A destination the dialog would refuse with a red path error: inside a
 *   source, or a move into the folder the items already sit in. Skipping would
 *   either fail later or do nothing, silently.
 * - S3 on either side: the dialog's cost line is the only place the price
 *   shows.
 * - Anything an agent started over MCP: its contract is the dialog (an
 *   auto-confirm confirms it, a plain call waits for `dialog confirm`).
 */

import { getPathValidationError } from '$lib/file-operations/transfer/transfer-dialog-logic'
import { initialEditedPath } from '$lib/file-operations/transfer/transfer-dialog-utils'
import { validateDirectoryPath } from '$lib/utils/filename-validation'
import { capabilitiesFor, capabilitiesForInfo } from './volume-capabilities'
import type { DeleteDialogPropsData, TransferConfirmPayload } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { VolumeInfo } from '../types'

/** What a skipped delete confirmation answers: `handleDeleteConfirm`'s arguments. */
export interface DeleteConfirmAnswer {
  previewId: null
  isPermanent: false
}

function isFromAgent(props: { initiator?: string; autoConfirm?: boolean; mcpRequestId?: string }): boolean {
  return props.initiator === 'aiClient' || props.autoConfirm === true || props.mcpRequestId !== undefined
}

function isS3(volumeId: string, volumes: readonly VolumeInfo[]): boolean {
  const info = volumes.find((v) => v.id === volumeId)
  return (info ? capabilitiesForInfo(info) : capabilitiesFor(volumeId)).kind === 's3'
}

/** The confirm a skipped transfer dialog gives, or `null` when the dialog must show. */
export function skippedTransferConfirmation(
  props: TransferDialogPropsData,
  volumes: readonly VolumeInfo[],
): TransferConfirmPayload | null {
  const { operationType, sourcePaths } = props
  if (operationType !== 'copy' && operationType !== 'move') return null
  if (props.newName !== undefined || isFromAgent(props)) return null
  if (isS3(props.sourceVolumeId, volumes) || isS3(props.currentVolumeId, volumes)) return null

  const volumePath = volumes.find((v) => v.id === props.currentVolumeId)?.path ?? '/'
  const destination = initialEditedPath(
    operationType,
    props.destinationPath,
    volumePath,
    sourcePaths,
    props.sourceFolderPath,
  )
  if (validateDirectoryPath(destination).severity === 'error') return null
  // Checked against both spellings: the path box holds the volume-relative one, and
  // the sources carry the pane's own, which on a volume mounted below `/` is absolute.
  if (getPathValidationError(sourcePaths, destination, operationType) !== null) return null
  if (getPathValidationError(sourcePaths, props.destinationPath, operationType) !== null) return null

  return {
    destination,
    volumeId: props.currentVolumeId,
    previewId: null,
    conflictResolution: 'stop',
    operationType,
    preKnownConflicts: [],
  }
}

/** The confirm a skipped delete dialog gives (always a trash), or `null` when it must show. */
export function skippedDeleteConfirmation(props: DeleteDialogPropsData): DeleteConfirmAnswer | null {
  if (props.isPermanent || !props.supportsTrash || props.isArchive === true) return null
  if (props.cloudOnlineOnly != null || props.cloudFolderMayHoldOnlineOnly === true) return null
  if (isFromAgent(props)) return null
  return { previewId: null, isPermanent: false }
}
