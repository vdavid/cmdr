import {
  DEFAULT_VOLUME_ID,
  copyFilesToClipboard,
  cutFilesToClipboard,
  copyPathsToClipboard,
  cutPathsToClipboard,
  readClipboardFiles,
  clearClipboardCutState,
} from '$lib/tauri-commands'
import { addToast, addToastForPane } from '$lib/ui/toast'
import { resolveSnapshotPaths } from '$lib/search/snapshot-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { tString } from '$lib/intl/messages.svelte'
import type { TransferOperationType } from '../types'
import { getCommonParentPath } from './transfer-operations'
import { checkTransferDestinationGuard } from './transfer-entry'
import { capabilitiesFor, capabilitiesForPane } from './volume-capabilities'
import { pasteClipboardContentAsFile } from './paste-clipboard-as-file'
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
 * Resolves the file/folder split from `readClipboardFiles`'s per-path kind
 * flags. Returns the per-type counts only when the flags are present, length-
 * aligned with the paths, and every entry is known (no `null`). Otherwise
 * returns `null`, which the caller threads as omitted counts so the completion
 * toast falls back to the flattened file-count wording. All-or-nothing on
 * purpose: a partial split would misreport.
 */
export function splitClipboardKinds(
  isDirectory: (boolean | null)[] | undefined,
  pathCount: number,
): { fileCount: number; folderCount: number } | null {
  if (isDirectory === undefined || isDirectory.length !== pathCount) return null
  if (isDirectory.some((flag) => flag === null)) return null

  let fileCount = 0
  let folderCount = 0
  for (const isDir of isDirectory) {
    if (isDir) folderCount += 1
    else fileCount += 1
  }
  return { fileCount, folderCount }
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
    const path = access.getPanePath(access.getFocusedPane())

    return { listingId, hasParent, selectedIndices, cursorIndex, volumeId, path }
  }

  /**
   * True when the focused pane is inside an archive (kind-from-path). The system
   * clipboard can't carry archive-inner paths (they aren't OS-resolvable files),
   * so ⌘C/⌘X are refused and the user is pointed at F5/F6 extract-out — the same
   * shape as the MTP refusal, but a different reason and hint.
   */
  function isArchivePane(volumeId: string, path: string): boolean {
    return capabilitiesForPane(volumeId, path).kind === 'archive'
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
        addToast(tString('fileExplorer.clipboard.copied', { countText: formatNumber(count), count }), { level: 'info' })
      } catch (error) {
        log.error('Clipboard copy from snapshot failed: {error}', { error })
      }
      return
    }

    const state = getClipboardPaneState()
    if (!state) return

    if (isArchivePane(state.volumeId, state.path)) {
      addToast(tString('fileExplorer.archive.useTransferToCopyOut'), { level: 'info' })
      return
    }

    if (isMtpClipboardRefusal(state.volumeId)) {
      addToast(tString('fileExplorer.clipboard.useF5FromMtp'), { level: 'info' })
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
      addToast(tString('fileExplorer.clipboard.copied', { countText: formatNumber(count), count }), { level: 'info' })
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
        addToast(tString('fileExplorer.clipboard.cutReady', { countText: formatNumber(count), count }), {
          level: 'info',
        })
      } catch (error) {
        log.error('Clipboard cut from snapshot failed: {error}', { error })
      }
      return
    }

    const state = getClipboardPaneState()
    if (!state) return

    if (isArchivePane(state.volumeId, state.path)) {
      addToast(tString('fileExplorer.archive.useTransferToCopyOut'), { level: 'info' })
      return
    }

    if (isMtpClipboardRefusal(state.volumeId)) {
      addToast(tString('fileExplorer.clipboard.useF6FromMtp'), { level: 'info' })
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
      addToast(tString('fileExplorer.clipboard.cutReady', { countText: formatNumber(count), count }), {
        level: 'info',
      })
    } catch (error) {
      log.error('Clipboard cut failed: {error}', { error })
    }
  }

  /**
   * The no-file-URLs branch of paste: gathers the focused pane's destination
   * state and runs `pasteClipboardContentAsFile` (create a file from text/image/
   * PDF content, or replicate today's warn toast when nothing is created).
   */
  async function runContentPasteFallback() {
    const focused = access.getFocusedPane()
    const paneRef = access.getPaneRef(focused)
    await pasteClipboardContentAsFile({
      volumeId: access.getPaneVolumeId(focused),
      directory: access.getPanePath(focused),
      listingId: paneRef?.getListingId() ?? '',
      hasParent: paneRef?.hasParentEntry() ?? false,
      showHiddenFiles: access.getShowHiddenFiles(),
      paneRef,
      // Paste-as-file feedback describes the focused pane's directory, so tag it.
      originPane: focused,
      onNothingCreated: () => addToastForPane(focused, tString('fileExplorer.clipboard.empty'), { level: 'warn' }),
    })
  }

  /** Pastes files from the system clipboard into the current directory. */
  async function pasteFromClipboard(forceMove: boolean) {
    try {
      // Check MTP before reading clipboard; MTP paste is always rejected,
      // no point reading the system clipboard just to reject it. The capability
      // decides the refusal, not a `startsWith('mtp-')` string (A6). The
      // MTP-specific copy ("Use F5…") stays separate from the shared guard
      // because it points the user at the F5/F6 flow MTP paste lacks.
      const focused = access.getFocusedPane()
      const volumeId = access.getPaneVolumeId(focused)
      if (isMtpClipboardRefusal(volumeId)) {
        addToastForPane(focused, tString('fileExplorer.clipboard.useF5ToMtp'), { level: 'info' })
        return
      }

      // Shared destination guard (search-results refusal + archive/read-only
      // alert) — the same chain F5/F6 and drag-and-drop run, so pasting into a
      // read-only or archive destination gets the same alert instead of silently
      // queueing a transfer the backend would reject. The dest path drives the
      // archive kind-from-path check.
      const destPath = access.getPanePath(focused)
      const guard = checkTransferDestinationGuard(volumeId, access.getVolumes(), destPath)
      if (!guard.ok) {
        if (guard.toast) addToastForPane(focused, guard.toast.message, { level: guard.toast.level })
        else dialogs.showAlert(guard.alert.title, guard.alert.message)
        return
      }

      const result = await readClipboardFiles()

      if (result.paths.length === 0) {
        // No file URLs on the clipboard. Fall back to the "paste content as a
        // file" flow (gated by `fileOperations.pasteClipboardAsFile`).
        await runContentPasteFallback()
        return
      }

      const operationType: TransferOperationType = result.isCut || forceMove ? 'move' : 'copy'
      const { sortBy, sortOrder } = access.getPaneSort(access.getFocusedPane())
      const destVolId = access.getPaneVolumeId(access.getFocusedPane())
      const sourceFolderPath = getCommonParentPath(result.paths)

      // Per-type top-level split for the completion toast ("Copied 1 file and 2
      // folders"). `readClipboardFiles` returns each path's kind. We surface the
      // split only when EVERY flag is known; any `null` (stat failed) drops both
      // counts so the composer falls back to the flattened file-count wording.
      const split = splitClipboardKinds(result.isDirectory, result.paths.length)

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
        fileCount: split?.fileCount,
        folderCount: split?.folderCount,
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
