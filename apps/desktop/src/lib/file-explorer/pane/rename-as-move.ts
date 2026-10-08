/**
 * A rename too big to start unasked, handed to the Move dialog.
 *
 * On S3 a folder (or a big file) has no rename: the backend copies every object
 * and deletes the source. Past the small-rename count the inline editor doesn't
 * start that on its own; it closes and opens the Move dialog in rename mode,
 * prefilled with the new name, so the user sees the file and size counts and
 * confirms a move they can pause or cancel (`rename/DETAILS.md`).
 */

import { operationStartIsBlocked } from './operation-start-gate'
import type { RenameAsMoveRequest } from './rename-flow.svelte'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'
import type { TransferDialogPropsData } from './transfer-operations'

type DialogState = ReturnType<typeof createDialogState>

/** The Move dialog's props for renaming `request.sourcePath` to `request.newName` in place. */
export function renameAsMoveDialogProps(
  pane: 'left' | 'right',
  volumeId: string,
  sort: { sortBy: TransferDialogPropsData['sortColumn']; sortOrder: TransferDialogPropsData['sortOrder'] },
  request: RenameAsMoveRequest,
): TransferDialogPropsData {
  return {
    operationType: 'move',
    sourcePaths: [request.sourcePath],
    destinationPath: request.parentPath,
    // `direction` names the DESTINATION pane, and confirm derives the source pane
    // as its opposite. Here both are this pane; naming the other one keeps the
    // source side (selection snapshot, post-move refresh) on this pane, and a
    // move refreshes both sides anyway.
    direction: pane === 'left' ? 'right' : 'left',
    currentVolumeId: volumeId,
    fileCount: request.isDirectory ? 0 : 1,
    folderCount: request.isDirectory ? 1 : 0,
    sourceFolderPath: request.parentPath,
    sortColumn: sort.sortBy,
    sortOrder: sort.sortOrder,
    sourceVolumeId: volumeId,
    destVolumeId: volumeId,
    // A rename lands under a new name by construction, so it's never a duplicate.
    duplicateFollowUp: 'nothing',
    newName: request.newName,
  }
}

/** Opens the Move dialog in rename mode for a rename on `pane`. */
export function openRenameAsMove(
  access: PaneAccess,
  dialogs: DialogState,
  pane: 'left' | 'right',
  request: RenameAsMoveRequest,
): void {
  // The same gate F6 takes: an MCP rename can land while another dialog is up.
  if (operationStartIsBlocked()) return
  dialogs.showTransfer(renameAsMoveDialogProps(pane, access.getPaneVolumeId(pane), access.getPaneSort(pane), request))
}
