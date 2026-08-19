/**
 * The Duplicate command's body: one copy of each selected item, landing in the
 * folder it already lives in.
 *
 * It lives beside `duplicate-rename.ts` (the other half of duplicating in place)
 * rather than inside `file-operation-commands.ts`, because it is the one transfer
 * command with no dialog to open: it describes a transfer with the same builders
 * F5 uses and dispatches it straight to the progress dialog.
 */

import { addToast } from '$lib/ui/toast'
import {
  buildTransferPropsFromSelection,
  buildTransferPropsFromCursor,
  type TransferContext,
} from './transfer-operations'
import { checkTransferDestinationGuard } from './transfer-entry'
import { operationStartIsBlocked } from './operation-start-gate'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'

type DialogState = ReturnType<typeof createDialogState>

/**
 * The Duplicate command: copies the focused pane's selection (or the item under
 * its cursor) into the folder it already lives in, where the backend gives each
 * copy a free ` (N)` name.
 *
 * Two things it deliberately doesn't do:
 *
 * - **No confirmation dialog.** There's no destination to pick and no conflict to
 *   answer (a self-collision resolves before the conflict machinery is consulted),
 *   so it goes straight to the progress dialog the way paste does.
 * - **No rename editor**, hence `duplicateFollowUp: 'nothing'`. ⌘D is Finder's
 *   Duplicate and the familiarity that justifies the key rests on it asking
 *   nothing; it would also break stamping out copies in a row.
 *   `$lib/file-operations/transfer/DETAILS.md` § "Only paste and F5 end a
 *   duplicate in the rename editor".
 *
 * The shared destination guard runs against the pane's OWN folder, since source
 * and destination are the same place here: a read-only volume gets the same
 * alert F5 gives it, and a search-results pane the same refusal.
 */
export async function duplicateInPlace(access: PaneAccess, dialogs: DialogState): Promise<void> {
  // Nothing new starts behind a dialog, same gate as F5/F6: this command reaches
  // us from the native menu, whose items stay clickable whatever is on screen.
  if (operationStartIsBlocked()) return

  const pane = access.getFocusedPane()
  const paneRef = access.getPaneRef(pane)
  const folderPath = access.getPanePath(pane)
  const volumeId = access.getPaneVolumeId(pane)

  const guard = checkTransferDestinationGuard(volumeId, access.getVolumes(), folderPath)
  if (!guard.ok) {
    if (guard.toast) addToast(guard.toast.message, { level: guard.toast.level })
    else dialogs.showAlert(guard.alert.title, guard.alert.message)
    return
  }

  const listingId = paneRef?.getListingId()
  if (!listingId) return

  const hasParent = paneRef?.hasParentEntry() ?? false
  const selectedIndices = paneRef?.getSelectedIndices() ?? []
  const { sortBy, sortOrder } = access.getPaneSort(pane)
  // One folder plays both parts. That is the whole operation, and it's what lets
  // the same builders F5 uses describe it.
  const context: TransferContext = {
    showHiddenFiles: access.getShowHiddenFiles(),
    sourcePath: folderPath,
    destPath: folderPath,
    sourceVolumeId: volumeId,
    destVolumeId: volumeId,
    sortColumn: sortBy,
    sortOrder,
  }

  const isLeft = pane === 'left'
  const props =
    selectedIndices.length > 0
      ? await buildTransferPropsFromSelection('copy', listingId, selectedIndices, hasParent, isLeft, context)
      : await buildTransferPropsFromCursor('copy', listingId, paneRef, hasParent, isLeft, context)
  // Nothing under the cursor (a `..` row, an empty listing) is nothing to duplicate.
  if (!props) return

  // `props.direction` names the OTHER pane, which is right for a transfer across
  // the panes and wrong for this one: the copy lands where it came from, so both
  // sides are the focused pane.
  dialogs.startTransferProgress({
    operationType: 'copy',
    sourcePaths: props.sourcePaths,
    sourceFolderPath: folderPath,
    sourcePaneSide: pane,
    destinationPath: folderPath,
    direction: pane,
    sortColumn: props.sortColumn,
    sortOrder: props.sortOrder,
    previewId: null,
    sourceVolumeId: volumeId,
    destVolumeId: volumeId,
    fileCount: props.fileCount,
    folderCount: props.folderCount,
    duplicateFollowUp: 'nothing',
  })
}
