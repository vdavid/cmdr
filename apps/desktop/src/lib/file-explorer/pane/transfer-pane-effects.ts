/**
 * Everything a settled transfer does to the PANES, and the only module that can
 * do it.
 *
 * Every effect here reads birth context (which pane started the operation, in
 * which folder, of which type), so it exists once, bound to the birth slot. An
 * adopted view is built without it — that's what makes "an adopted view's
 * outcome handlers touch no pane" structural rather than a convention:
 * `adopted-operation.svelte.ts` has no reference to reach the pane work with.
 * `DETAILS.md` § "Birth context".
 */

import { refreshListing } from '$lib/tauri-commands'
import type { TransferOperationType } from '../types'
import type { FilePaneAPI } from './types'
import type { TransferProgressPropsData } from './dialog-props'

/** The pane handles the effects run against. A subset of `DialogStateDeps`. */
export interface TransferPaneEffectsDeps {
  getLeftPaneRef: () => FilePaneAPI | undefined
  getRightPaneRef: () => FilePaneAPI | undefined
}

/** Force a backend re-read on a pane's listing so file diffs are emitted promptly. */
function refreshPaneListing(paneRef: FilePaneAPI | undefined): void {
  const listingId = paneRef?.getListingId()
  if (listingId) void refreshListing(listingId)
}

/**
 * Builds the pane effects for one dialog state. `getBirthProps` reads the live
 * birth slot, so the effects always act on the operation currently in flight
 * and are inert (no pane touched) once it is settled and the slot is empty.
 */
export function createTransferPaneEffects(
  deps: TransferPaneEffectsDeps,
  getBirthProps: () => TransferProgressPropsData | null,
) {
  function getSourcePaneRef(): FilePaneAPI | undefined {
    return getBirthProps()?.sourcePaneSide === 'left' ? deps.getLeftPaneRef() : deps.getRightPaneRef()
  }

  /**
   * Whether the birth context still describes the source pane.
   *
   * The axis for a view's pane work is FRESH versus STALE context, not "did this
   * view start the operation": a pane that navigated away mid-transfer holds a
   * selection the user made somewhere else, and the archive-password re-dispatch
   * re-snapshots against wherever the pane is now. Refreshing a listing is
   * harmless either way, but changing a selection is not, so the selection work
   * asks this first.
   *
   * Comparing the pane's current folder to the one the operation was born in is
   * the honest cheap test. No pane at all means nothing to speak for.
   */
  function sourcePaneStillShowsBirthFolder(): boolean {
    const props = getBirthProps()
    const paneRef = getSourcePaneRef()
    if (!props || !paneRef) return false
    return paneRef.getCurrentPath() === props.sourceFolderPath
  }

  return {
    /** Takes the source pane's selection snapshot for the operation about to run. */
    snapshotSourcePaneSelection(): void {
      void getSourcePaneRef()?.snapshotSelectionForOperation()
    },

    /** Drops the source pane's operation snapshot and its selection, the tail every
     *  settled transfer runs. Skipped for a pane that has navigated since the
     *  operation was born: the selection there is one the user made somewhere
     *  else, and this operation has no business clearing it. */
    clearSourcePaneAfterTransfer(): void {
      if (!sourcePaneStillShowsBirthFolder()) return
      getSourcePaneRef()?.clearOperationSnapshot()
      getSourcePaneRef()?.clearSelection()
    },

    /** Adjusts source pane selection after a cancelled operation based on the snapshot state. */
    adjustSelectionAfterCancel(op: TransferOperationType): void {
      // Same rule as `clearSourcePaneAfterTransfer`: a pane showing a different
      // folder has a selection that isn't this operation's to restore or clear.
      if (!sourcePaneStillShowsBirthFolder()) return
      const prevSnapshot = getSourcePaneRef()?.clearOperationSnapshot()
      if (prevSnapshot === 'all' && op !== 'copy' && op !== 'compress') {
        // Re-select all survivors (move/delete/trash changed the source listing;
        // copy and compress leave the source listing intact, so indices still hold)
        getSourcePaneRef()?.selectAll()
      } else if (prevSnapshot == null) {
        // No snapshot was taken, so there's nothing to restore: clear.
        getSourcePaneRef()?.clearSelection()
      }
      // For 'all' + copy: source listing unchanged, existing indices still valid
      // For array snapshot: selection already reflects survivors from diff-driven adjustment
    },

    /** Refreshes panes after a transfer settles. For move/delete/trash, refresh both panes. */
    refreshPanesAfterTransfer(): void {
      const props = getBirthProps()
      const opType = props?.operationType
      const isDeleteOrTrash = opType === 'delete' || opType === 'trash'

      if (isDeleteOrTrash) {
        // Delete/trash: refresh both panes (both might show the affected directory)
        refreshPaneListing(deps.getLeftPaneRef())
        refreshPaneListing(deps.getRightPaneRef())
      } else {
        const destPaneRef = props?.direction === 'right' ? deps.getRightPaneRef() : deps.getLeftPaneRef()
        const sourcePaneRef = props?.direction === 'right' ? deps.getLeftPaneRef() : deps.getRightPaneRef()

        // Force backend to re-read directories and emit diffs. The file watcher may
        // not have fired yet (common for instant renames on Linux), leaving stale cache.
        refreshPaneListing(destPaneRef)
        if (opType === 'move') {
          refreshPaneListing(sourcePaneRef)
        }
      }

      // Refresh disk space on both panes (both might be on the same volume)
      void deps.getLeftPaneRef()?.refreshVolumeSpace()
      void deps.getRightPaneRef()?.refreshVolumeSpace()
    },
  }
}
