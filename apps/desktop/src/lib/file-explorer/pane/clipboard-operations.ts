import {
  DEFAULT_VOLUME_ID,
  copyFilesToClipboard,
  cutFilesToClipboard,
  copyPathsToClipboard,
  cutPathsToClipboard,
  readClipboardFiles,
  clearClipboardCutState,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { resolveSnapshotPaths } from '$lib/search/snapshot-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import type { TransferOperationType } from '../types'
import { getCommonParentPath } from './transfer-operations'
import { capabilitiesFor } from './volume-capabilities'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'

const log = getAppLogger('fileExplorer')

type DialogState = ReturnType<typeof createDialogState>

/**
 * True when the focused pane is an MTP device, which can't use the system
 * clipboard (virtual paths can't go on the OS clipboard) — the copy/cut/paste
 * refusal that points the user at F5/F6 instead.
 *
 * Reads the kind off the capability table rather than a `startsWith('mtp-')`
 * string compare (invariant A6). The MTP kind is what carries
 * `supportsSystemClipboard: false`; we key on `kind === 'mtp'` rather than the
 * raw flag because `network` and `search-results` ALSO lack a system clipboard,
 * and an MTP-worded toast firing on a (reachable) network paste would be a new,
 * mis-worded toast (PR3). On the live clipboard-time pane id set this is
 * byte-equivalent to the old `volumeId.startsWith('mtp-')` gate — live MTP panes
 * carry `mtp-{…}` ids, which classify to `kind === 'mtp'`; nothing else does
 * (pinned by the equivalence test in `clipboard-operations.test.ts`).
 */
function isMtpClipboardRefusal(volumeId: string): boolean {
  return capabilitiesFor(volumeId).kind === 'mtp'
}

/**
 * System-clipboard copy / cut / paste for the focused pane. Lifted out of
 * `DualPaneExplorer` so the MTP-refusal, snapshot-pane, and cut-vs-copy branches
 * are headless-testable. Reads pane state through `PaneAccess`; opens the paste
 * transfer through the explorer's dialog state.
 */
export function createClipboardOperations(access: PaneAccess, dialogs: DialogState) {
  /** Gathers pane state needed for clipboard copy/cut. Returns null if unavailable. */
  function getClipboardPaneState() {
    const sourcePaneRef = access.getPaneRef(access.getFocusedPane())
    const listingId = sourcePaneRef?.getListingId()
    if (!listingId) return null

    const hasParent = sourcePaneRef?.hasParentEntry() ?? false
    const selectedIndices = sourcePaneRef?.getSelectedIndices() ?? []
    const cursorIndex = sourcePaneRef?.getCursorIndex() ?? 0
    const volumeId = access.getPaneVolumeId(access.getFocusedPane())

    return { listingId, hasParent, selectedIndices, cursorIndex, volumeId }
  }

  /**
   * Search-results pane copy/cut path: resolve the focused pane's cursor +
   * selection into snapshot paths and feed the paths-by-value clipboard IPCs.
   * Returns the resolved paths and snapshot id so the caller can write a friendly
   * toast, or `null` if the pane isn't a snapshot pane or the snapshot is missing
   * / empty.
   */
  function getSnapshotClipboardPaths(): { paths: string[]; snapshotId: string } | null {
    const focusedVolId = access.getPaneVolumeId(access.getFocusedPane())
    // The snapshot-clip path applies to the search-results namespace. Read the
    // pane's path scheme off the capability table rather than a
    // `volumeId === 'search-results'` string compare (A6).
    if (capabilitiesFor(focusedVolId).pathScheme !== 'search-results') return null
    const sourcePaneRef = access.getPaneRef(access.getFocusedPane())
    const currentPath = sourcePaneRef?.getCurrentPath() ?? ''
    // Extract the snapshot id from the URL — pure namespace mechanics, kept as-is.
    const SEARCH_RESULTS_PREFIX = 'search-results://'
    if (!currentPath.startsWith(SEARCH_RESULTS_PREFIX)) return null
    const snapshotId = currentPath.slice(SEARCH_RESULTS_PREFIX.length)
    const selectedIndices = sourcePaneRef?.getSelectedIndices() ?? []
    const cursorIndex = sourcePaneRef?.getCursorIndex() ?? 0
    const paths = resolveSnapshotPaths(snapshotId, selectedIndices, cursorIndex)
    if (paths.length === 0) return null
    return { paths, snapshotId }
  }

  /** Copies selected files (or cursor file) to the system clipboard. */
  async function copyToClipboard() {
    // Search-results pane: paths are already absolute on the snapshot. The
    // regular listing-id path can't apply because there's no backend listing.
    const snapshotClip = getSnapshotClipboardPaths()
    if (snapshotClip) {
      try {
        const count = await copyPathsToClipboard(snapshotClip.paths)
        addToast(`Copied ${formatNumber(count)} ${count === 1 ? 'item' : 'items'}`, { level: 'info' })
      } catch (error) {
        log.error('Clipboard copy from snapshot failed: {error}', { error })
      }
      return
    }

    const state = getClipboardPaneState()
    if (!state) return

    if (isMtpClipboardRefusal(state.volumeId)) {
      addToast('Use F5 to copy files from MTP devices', { level: 'info' })
      return
    }

    try {
      const count = await copyFilesToClipboard(
        state.listingId,
        state.selectedIndices,
        state.cursorIndex,
        state.hasParent,
        access.getShowHiddenFiles(),
      )
      addToast(`Copied ${formatNumber(count)} ${count === 1 ? 'item' : 'items'}`, { level: 'info' })
    } catch (error) {
      log.error('Clipboard copy failed: {error}', { error })
    }
  }

  /** Cuts selected files (or cursor file) to the system clipboard. */
  async function cutToClipboard() {
    const snapshotClip = getSnapshotClipboardPaths()
    if (snapshotClip) {
      try {
        const count = await cutPathsToClipboard(snapshotClip.paths)
        addToast(`${formatNumber(count)} ${count === 1 ? 'item' : 'items'} ready to move. Paste to complete.`, {
          level: 'info',
        })
      } catch (error) {
        log.error('Clipboard cut from snapshot failed: {error}', { error })
      }
      return
    }

    const state = getClipboardPaneState()
    if (!state) return

    if (isMtpClipboardRefusal(state.volumeId)) {
      addToast('Use F6 to move files from MTP devices', { level: 'info' })
      return
    }

    try {
      const count = await cutFilesToClipboard(
        state.listingId,
        state.selectedIndices,
        state.cursorIndex,
        state.hasParent,
        access.getShowHiddenFiles(),
      )
      addToast(`${formatNumber(count)} ${count === 1 ? 'item' : 'items'} ready to move. Paste to complete.`, {
        level: 'info',
      })
    } catch (error) {
      log.error('Clipboard cut failed: {error}', { error })
    }
  }

  /** Pastes files from the system clipboard into the current directory. */
  async function pasteFromClipboard(forceMove: boolean) {
    try {
      // Check MTP before reading clipboard; MTP paste is always rejected,
      // no point reading the system clipboard just to reject it. The capability
      // decides the refusal, not a `startsWith('mtp-')` string (A6).
      const volumeId = access.getPaneVolumeId(access.getFocusedPane())
      if (isMtpClipboardRefusal(volumeId)) {
        addToast('Use F5 to copy files to MTP devices', { level: 'info' })
        return
      }

      const result = await readClipboardFiles()

      if (result.paths.length === 0) {
        addToast('No files on the clipboard. Copy files first with ⌘C.', { level: 'warn' })
        return
      }

      const operationType: TransferOperationType = result.isCut || forceMove ? 'move' : 'copy'
      const destPath = access.getPanePath(access.getFocusedPane())
      const { sortBy, sortOrder } = access.getPaneSort(access.getFocusedPane())
      const destVolId = access.getPaneVolumeId(access.getFocusedPane())
      const sourceFolderPath = getCommonParentPath(result.paths)

      dialogs.startTransferProgress({
        operationType,
        sourcePaths: result.paths,
        sourceFolderPath,
        // Clipboard files don't belong to a specific pane; pick the opposite as best guess.
        // Harmless if wrong: it just clears selection on the non-destination pane.
        sourcePaneSide: access.getFocusedPane() === 'left' ? 'right' : 'left',
        destinationPath: destPath,
        direction: access.getFocusedPane() === 'left' ? 'left' : 'right',
        sortColumn: sortBy,
        sortOrder,
        previewId: null,
        sourceVolumeId: DEFAULT_VOLUME_ID,
        destVolumeId: destVolId,
      })

      if (result.isCut) {
        void clearClipboardCutState()
      }
    } catch (error) {
      log.error('Clipboard paste failed: {error}', { error })
    }
  }

  return { copyToClipboard, cutToClipboard, pasteFromClipboard, getSnapshotClipboardPaths }
}
