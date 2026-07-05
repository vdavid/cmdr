import { DEFAULT_VOLUME_ID, getFileAt, getFilesAtIndices } from '$lib/tauri-commands'
import { pluralize } from '$lib/utils/pluralize'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getSnapshot } from '$lib/search/snapshot-store.svelte'
import { openFileViewer } from '$lib/file-viewer/open-viewer'
import { getAppLogger } from '$lib/logging/logger'
import { toBackendCursorIndex, toBackendIndices } from '$lib/file-operations/transfer/transfer-dialog-utils'
import { getInitialFolderName } from '$lib/file-operations/mkdir/new-folder-operations'
import { getInitialFileName } from '$lib/file-operations/mkfile/new-file-operations'
import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
import {
  type TransferContext,
  buildTransferPropsFromSelection,
  buildTransferPropsFromCursor,
  buildTransferPropsFromSnapshot,
  getDestinationVolumeInfo,
} from './transfer-operations'
import { capabilitiesFor, pathInsideArchive } from './volume-capabilities'
import { checkTransferDestinationGuard } from './transfer-entry'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { FilePaneAPI } from './types'
import type { TransferOperationType } from '../types'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'

const log = getAppLogger('fileExplorer')

type DialogState = ReturnType<typeof createDialogState>

/**
 * Rename, new-folder / new-file, viewer, transfer (copy / move), and delete
 * command bodies for the focused pane. Lifted out of `DualPaneExplorer` so the
 * read-only-volume, search-results-destination, and snapshot-pane guard chains
 * are headless-testable. Reads pane state through `PaneAccess`; opens dialogs
 * through the explorer's dialog state.
 */
export function createFileOperationCommands(access: PaneAccess, dialogs: DialogState) {
  /**
   * The read-only refusal alert for a write action on the focused pane, or `null`
   * when the pane accepts writes. A zip archive is WRITABLE (the pane's `volumeId`
   * is the parent drive and `capabilitiesForPane` gives the writable `archive`
   * row), so an archive pane falls through here and runs the real managed
   * archive-edit flow. What still refuses is a read-only `VolumeInfo` (a
   * write-protected USB stick, a read-only disk image) — including a zip that
   * lives on such a volume, which can't be rewritten in place. Surfacing this up
   * front beats letting the user type a name and then hit a backend rejection.
   */
  function readOnlyRefusal(
    action: 'rename' | 'mkdir' | 'mkfile' | 'delete',
  ): { title: string; message: string } | null {
    const pane = access.getFocusedPane()
    const volId = access.getPaneVolumeId(pane)

    const volumeInfo = getDestinationVolumeInfo(volId, access.getVolumes())
    if (volumeInfo?.isReadOnly) {
      const messageKey: Record<typeof action, MessageKey> = {
        rename: 'fileExplorer.readOnly.renameMessage',
        mkdir: 'fileExplorer.readOnly.mkdirMessage',
        mkfile: 'fileExplorer.readOnly.mkfileMessage',
        delete: 'fileExplorer.readOnly.deleteMessage',
      }
      return { title: tString('fileExplorer.readOnly.volumeTitle'), message: tString(messageKey[action]) }
    }

    return null
  }

  /** Activates inline rename on the focused pane's cursor item. */
  function startRename() {
    const refusal = readOnlyRefusal('rename')
    if (refusal) {
      dialogs.showAlert(refusal.title, refusal.message)
      return
    }

    const paneRef = access.getPaneRef(access.getFocusedPane())
    paneRef?.startRename()
  }

  /** Cancels any active inline rename on either pane. */
  function cancelRename() {
    for (const side of ['left', 'right'] as const) {
      access.getPaneRef(side)?.cancelRename()
    }
  }

  /** Returns whether inline rename is active on either pane. */
  function isRenaming(): boolean {
    return (['left', 'right'] as const).some((side) => {
      return access.getPaneRef(side)?.isRenaming()
    })
  }

  /** Opens the new folder dialog. Pre-fills with the entry name under cursor. */
  async function openNewFolderDialog() {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    const path = access.getPanePath(access.getFocusedPane())
    const volumeIdForPane = access.getPaneVolumeId(access.getFocusedPane())

    // Read-only destinations (a write-protected volume, or inside an archive) can't
    // accept new folders. Surface that as an alert up front rather than letting the
    // user type a name and then hit a backend rejection. Mirrors `startRename`.
    const refusal = readOnlyRefusal('mkdir')
    if (refusal) {
      dialogs.showAlert(refusal.title, refusal.message)
      return
    }

    const paneListingId = paneRef?.getListingId()
    if (!paneListingId) {
      log.warn('openNewFolderDialog: no listingId, bailing')
      return
    }

    const initialName = await getInitialFolderName(paneRef, paneListingId, access.getShowHiddenFiles(), getFileAt)

    dialogs.showNewFolder({
      currentPath: path,
      listingId: paneListingId,
      showHiddenFiles: access.getShowHiddenFiles(),
      initialName,
      volumeId: volumeIdForPane,
    })
  }

  /** Opens the new file dialog. Pre-fills with the filename under cursor. */
  async function openNewFileDialog() {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    const path = access.getPanePath(access.getFocusedPane())
    const volumeIdForPane = access.getPaneVolumeId(access.getFocusedPane())

    const refusal = readOnlyRefusal('mkfile')
    if (refusal) {
      dialogs.showAlert(refusal.title, refusal.message)
      return
    }

    const paneListingId = paneRef?.getListingId()
    if (!paneListingId) {
      log.warn('openNewFileDialog: no listingId, bailing')
      return
    }

    const initialName = await getInitialFileName(paneRef, paneListingId, access.getShowHiddenFiles(), getFileAt)

    dialogs.showNewFile({
      currentPath: path,
      listingId: paneListingId,
      showHiddenFiles: access.getShowHiddenFiles(),
      initialName,
      volumeId: volumeIdForPane,
    })
  }

  /** Closes any confirmation dialog (new folder, new file, or transfer) if open (for MCP). */
  function closeConfirmationDialog() {
    dialogs.closeConfirmationDialog()
  }

  /** Returns whether any confirmation dialog is currently open. */
  function isConfirmationDialogOpen(): boolean {
    return dialogs.isConfirmationDialogOpen()
  }

  /** Opens the file viewer for the file under the cursor. */
  async function openViewerForCursor() {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    const listingId = paneRef?.getListingId()
    if (!listingId) return
    const cursorIndex = paneRef?.getCursorIndex()
    const hasParent = paneRef?.hasParentEntry()
    const backendIndex = toBackendCursorIndex(cursorIndex ?? -1, hasParent ?? false)
    if (backendIndex === null) return

    const file = await getFileAt(listingId, backendIndex, access.getShowHiddenFiles())
    if (!file || file.isDirectory || file.name === '..') return

    void openFileViewer(file.path)
  }

  /** Builds a TransferContext from pane state. */
  function buildTransferContext(pane: 'left' | 'right'): TransferContext {
    const other = access.otherPane(pane)
    const { sortBy, sortOrder } = access.getPaneSort(pane)
    return {
      showHiddenFiles: access.getShowHiddenFiles(),
      sourcePath: access.getPanePath(pane),
      destPath: access.getPanePath(other),
      sourceVolumeId: access.getPaneVolumeId(pane),
      destVolumeId: access.getPaneVolumeId(other),
      sortColumn: sortBy,
      sortOrder,
    }
  }

  /**
   * Builds transfer dialog props for a search-results source pane (M8d).
   * The snapshot view has no backend listing, so the listing-id-driven
   * builders don't apply; we read the snapshot directly and feed
   * absolute paths into `buildTransferPropsFromSnapshot`. Returns `null`
   * when there's no snapshot or nothing under the cursor / selection.
   *
   * `canBeSource: true` per the `search-results` capability row: source-side
   * operations always run against the real underlying files. After a move
   * completes, `dialog-state::handleTransferComplete` already purges moved
   * paths from every snapshot via `removeEntryFromAllSnapshots`.
   */
  function buildSnapshotTransferProps(
    operationType: TransferOperationType,
    sourcePaneRef: FilePaneAPI | undefined,
    pane: 'left' | 'right',
  ) {
    const currentPath = sourcePaneRef?.getCurrentPath() ?? ''
    const SEARCH_RESULTS_PREFIX = 'search-results://'
    if (!currentPath.startsWith(SEARCH_RESULTS_PREFIX)) return null
    const snapshotId = currentPath.slice(SEARCH_RESULTS_PREFIX.length)
    const snapshot = getSnapshot(snapshotId)
    if (!snapshot) return null

    const selectedIndices = sourcePaneRef?.getSelectedIndices() ?? []
    const cursorIndex = sourcePaneRef?.getCursorIndex() ?? 0
    const useIndices = selectedIndices.length > 0 ? selectedIndices : [cursorIndex]

    const sourcePaths: string[] = []
    const isDirectoryFlags: boolean[] = []
    for (const idx of useIndices) {
      // TS doesn't model array bounds (no `noUncheckedIndexedAccess`), so
      // `snapshot.entries[idx]` is typed as non-undefined. The guard is
      // still load-bearing at runtime: `selectedIndices` can carry stale
      // indices after a snapshot mutation (the M8c delete-sync rewrites
      // the entries array, but in-flight selections may briefly point
      // past the new end).

      const entry = snapshot.entries[idx]
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
      if (!entry) continue
      sourcePaths.push(entry.path)
      isDirectoryFlags.push(entry.isDirectory)
    }
    if (sourcePaths.length === 0) return null

    const other = access.otherPane(pane)
    const { sortBy, sortOrder } = access.getPaneSort(pane)
    return buildTransferPropsFromSnapshot(
      operationType,
      sourcePaths,
      isDirectoryFlags,
      pane === 'left',
      access.getPanePath(other),
      access.getPaneVolumeId(other),
      sortBy,
      sortOrder,
    )
  }

  /** Opens the unified transfer dialog for all volume types (local, MTP, search-results, etc.). */
  async function openUnifiedTransferDialog(
    operationType: TransferOperationType,
    sourcePaneRef: FilePaneAPI | undefined,
    pane: 'left' | 'right',
    autoConfirm?: boolean,
    onConflict?: string,
  ) {
    // Snapshot source pane: no backend listing exists, so the listing-id-driven
    // builders don't apply — build from the snapshot's selected (or cursor)
    // entries. Routed off the kind's `hasBackendListing` capability, not a
    // `volumeId === 'search-results'` string compare (A6). Among kinds that reach
    // this transfer opener, search-results is the only `!hasBackendListing` one
    // (a network pane can't be a transfer source — `canBeSource: false`).
    if (!capabilitiesFor(access.getPaneVolumeId(pane)).hasBackendListing) {
      const snapshotProps = buildSnapshotTransferProps(operationType, sourcePaneRef, pane)
      if (snapshotProps) {
        if (autoConfirm) {
          snapshotProps.autoConfirm = true
          snapshotProps.autoConfirmOnConflict = onConflict
        }
        dialogs.showTransfer(snapshotProps)
      }
      return
    }

    const listingId = sourcePaneRef?.getListingId()
    if (!listingId) return

    const hasParent = sourcePaneRef?.hasParentEntry()
    const selectedIndices = sourcePaneRef?.getSelectedIndices()
    const hasSelection = selectedIndices && selectedIndices.length > 0

    const context = buildTransferContext(pane)
    const isLeft = pane === 'left'

    const props = hasSelection
      ? await buildTransferPropsFromSelection(
          operationType,
          listingId,
          selectedIndices,
          hasParent ?? false,
          isLeft,
          context,
        )
      : await buildTransferPropsFromCursor(operationType, listingId, sourcePaneRef, hasParent ?? false, isLeft, context)

    if (props) {
      if (autoConfirm) {
        props.autoConfirm = true
        props.autoConfirmOnConflict = onConflict
      }
      dialogs.showTransfer(props)
    }
  }

  /** Opens the transfer dialog with the current selection info. */
  async function openTransferDialog(operationType: TransferOperationType, autoConfirm?: boolean, onConflict?: string) {
    const sourcePaneRef = access.getPaneRef(access.getFocusedPane())
    const destPane = access.otherPane(access.getFocusedPane())
    const destVolId = access.getPaneVolumeId(destPane)
    const destPath = access.getPanePath(destPane)

    // Shared destination guard chain (search-results refusal + archive/read-only
    // alert). Every transfer entry path — F5/F6, drag-and-drop, clipboard paste —
    // runs the same `checkTransferDestinationGuard`, so the refusal copy and
    // ordering can't drift between paths. `destPath` drives the archive
    // kind-from-path check. The F-key bar already disables F5/F6 when the OPPOSITE
    // pane is a snapshot, so the search-results branch here is a belt-and-braces
    // guard for the shortcut path.
    const guard = checkTransferDestinationGuard(destVolId, access.getVolumes(), destPath)
    if (!guard.ok) {
      if (guard.toast) addToast(guard.toast.message, { level: guard.toast.level })
      else dialogs.showAlert(guard.alert.title, guard.alert.message)
      return
    }

    await openUnifiedTransferDialog(operationType, sourcePaneRef, access.getFocusedPane(), autoConfirm, onConflict)
  }

  /** Opens the copy dialog (convenience wrapper for MCP/key binding). */
  async function openCopyDialog(autoConfirm?: boolean, onConflict?: string) {
    await openTransferDialog('copy', autoConfirm, onConflict)
  }

  /** Opens the move dialog (convenience wrapper for MCP/key binding). */
  async function openMoveDialog(autoConfirm?: boolean, onConflict?: string) {
    await openTransferDialog('move', autoConfirm, onConflict)
  }

  /**
   * Search-results pane delete path (M8c). The focused pane is on the
   * `search-results://<id>` virtual volume, so there's no backend listing to
   * fetch entries from; we read the snapshot directly. Today the snapshot
   * pane doesn't expose a multi-selection of its own, so we delete the
   * single cursor row. The volume id we report to the dialog is `'root'`:
   * the actual file lives on the local filesystem, and the existing
   * permanent-delete / move-to-trash IPC routes through the local path.
   * `supportsTrash = true` because the underlying file is on a trash-capable
   * volume (we don't have per-snapshot-row volume detection yet; if the
   * search ever indexes external read-only volumes we'd need to look that
   * up per entry).
   */
  function openDeleteFromSearchResults(permanent: boolean, autoConfirm?: boolean) {
    const sourcePaneRef = access.getPaneRef(access.getFocusedPane())
    const currentPath = sourcePaneRef?.getCurrentPath() ?? ''
    const SEARCH_RESULTS_PREFIX = 'search-results://'
    if (!currentPath.startsWith(SEARCH_RESULTS_PREFIX)) {
      log.warn('openDeleteFromSearchResults: focused pane volume is search-results but path is not. Bailing.')
      return
    }
    const snapshotId = currentPath.slice(SEARCH_RESULTS_PREFIX.length)
    const snapshot = getSnapshot(snapshotId)
    if (!snapshot) {
      log.warn('openDeleteFromSearchResults: snapshot {id} not found, bailing', { id: snapshotId })
      return
    }
    const cursorIndex = sourcePaneRef?.getCursorIndex() ?? 0
    // Cursor might be out of range (clamping is best-effort in the search-
    // results keyboard path); the cast lets us handle the empty case
    // explicitly instead of crashing later in `entry.path`.
    const entry = snapshot.entries[cursorIndex] as (typeof snapshot.entries)[number] | undefined
    if (!entry) {
      log.warn('openDeleteFromSearchResults: no entry at cursor {idx}, bailing', { idx: cursorIndex })
      return
    }

    const sourceItems: DeleteSourceItem[] = [
      {
        name: entry.name,
        size: entry.size ?? undefined,
        isDirectory: entry.isDirectory,
        isSymlink: false,
        recursiveSize: undefined,
        recursiveFileCount: undefined,
      },
    ]
    const sourcePaths = [entry.path]

    const { sortBy, sortOrder } = access.getPaneSort(access.getFocusedPane())

    // Snapshot entries are guaranteed to have parentPath set by the search
    // backend (`SearchResultEntry::parentPath` is required, see bindings).
    // The fallback isn't hit in practice, but `'/'` is a safe display
    // value if the field is ever absent.
    const sourceFolderPath = entry.parentPath !== '' ? entry.parentPath : '/'

    dialogs.showDeleteConfirmation({
      sourceItems,
      sourcePaths,
      sourceFolderPath,
      isPermanent: permanent,
      supportsTrash: true,
      isFromCursor: true,
      sortColumn: sortBy,
      sortOrder,
      sourceVolumeId: DEFAULT_VOLUME_ID,
      autoConfirm,
    })
  }

  /** Opens the delete confirmation dialog for the current selection or cursor item. */
  // eslint-disable-next-line complexity -- Guard chain: each early-return is an independent precondition; splitting wouldn't add clarity.
  async function openDeleteDialog(permanent: boolean, autoConfirm?: boolean) {
    const sourcePaneRef = access.getPaneRef(access.getFocusedPane())
    const focusedVolId = access.getPaneVolumeId(access.getFocusedPane())

    // Snapshot pane: no backend listing exists, so the listingId-driven path
    // can't fetch entries. Build the dialog directly from the snapshot's cursor
    // entry. Source-side delete is allowed (`canBeSource: true`): the underlying
    // file IS real, the confirmation dialog shows the real path, and on success
    // the entry is also removed from every other snapshot that contained it (see
    // `dialog-state::handleTransferComplete`). Routed off `hasBackendListing`,
    // not a `volumeId === 'search-results'` string compare (A6); search-results
    // is the only source-capable `!hasBackendListing` kind to reach here.
    if (!capabilitiesFor(focusedVolId).hasBackendListing) {
      openDeleteFromSearchResults(permanent, autoConfirm)
      return
    }

    const listingId = sourcePaneRef?.getListingId()
    if (!listingId) {
      log.warn('openDeleteDialog: no listingId, bailing')
      return
    }

    // Read-only sources (a write-protected volume, or inside an archive) can't
    // accept deletes. Surface as an alert before any dialog opens. Mirrors
    // `startRename` and `openNewFolderDialog`.
    const refusal = readOnlyRefusal('delete')
    if (refusal) {
      dialogs.showAlert(refusal.title, refusal.message)
      return
    }

    const hasParent = sourcePaneRef?.hasParentEntry()
    const selectedIndices = sourcePaneRef?.getSelectedIndices()
    const hasSelection = selectedIndices && selectedIndices.length > 0

    const backendIndices = hasSelection
      ? toBackendIndices(selectedIndices, hasParent ?? false)
      : (() => {
          const cursorIndex = sourcePaneRef?.getCursorIndex()
          const idx = toBackendCursorIndex(cursorIndex ?? -1, hasParent ?? false)
          return idx !== null ? [idx] : []
        })()
    if (backendIndices.length === 0) {
      log.warn(
        'openDeleteDialog: no backendIndices (hasSelection={hasSelection}, cursorIndex={cursorIndex}, hasParent={hasParent}), bailing',
        {
          hasSelection,
          cursorIndex: sourcePaneRef?.getCursorIndex() ?? -1,
          hasParent: hasParent ?? false,
        },
      )
      return
    }

    // Fetch full FileEntry data in a single batch IPC call
    let entries
    try {
      entries = await getFilesAtIndices(listingId, backendIndices, access.getShowHiddenFiles())
    } catch (error) {
      log.warn('openDeleteDialog: getFilesAtIndices threw, bailing. error={error}, indices={indices}', {
        error: error instanceof Error ? error.message : String(error),
        indices: backendIndices.join(','),
      })
      return
    }
    const validEntries = entries.filter((e) => e.name !== '..')
    if (validEntries.length === 0) {
      log.warn(
        'openDeleteDialog: no validEntries after getFilesAtIndices (got {count} {entriesNoun}, all filtered as ".." parent), bailing. backendIndices={indices}',
        {
          count: entries.length,
          entriesNoun: pluralize(entries.length, 'entry', 'entries'),
          indices: backendIndices.join(','),
        },
      )
      return
    }
    log.debug('openDeleteDialog: opening delete confirmation. {count} {entriesNoun}, sourceVolId={volId}', {
      count: validEntries.length,
      entriesNoun: pluralize(validEntries.length, 'valid entry', 'valid entries'),
      volId: access.getPaneVolumeId(access.getFocusedPane()),
    })

    const sourceItems: DeleteSourceItem[] = validEntries.map((e) => ({
      name: e.name,
      size: e.size,
      isDirectory: e.isDirectory,
      isSymlink: e.isSymlink,
      recursiveSize: e.recursiveSize,
      recursiveFileCount: e.recursiveFileCount,
    }))
    const sourcePaths = validEntries.map((e) => e.path)

    // Look up supportsTrash from the source volume
    const sourceVolId = access.getPaneVolumeId(access.getFocusedPane())
    const sourceFolderPath = access.getPanePath(access.getFocusedPane())
    const sourceVolume = access.getVolumes().find((v) => v.id === sourceVolId)
    // Deleting an entry INSIDE a zip is permanent: there's no Trash inside an
    // archive (the backend rejects trashing an archive-inner path), so force
    // permanent + the archive warning regardless of the parent drive's trash
    // support or the F8/Shift+F8 preselect.
    const sourceIsArchive = pathInsideArchive(sourceFolderPath)
    const supportsTrash = sourceIsArchive ? false : sourceVolume?.supportsTrash !== false

    const { sortBy, sortOrder } = access.getPaneSort(access.getFocusedPane())

    dialogs.showDeleteConfirmation({
      sourceItems,
      sourcePaths,
      sourceFolderPath,
      isPermanent: permanent || sourceIsArchive,
      supportsTrash,
      isArchive: sourceIsArchive,
      isFromCursor: !hasSelection,
      sortColumn: sortBy,
      sortOrder,
      sourceVolumeId: sourceVolId,
      autoConfirm,
    })
  }

  return {
    startRename,
    cancelRename,
    isRenaming,
    openNewFolderDialog,
    openNewFileDialog,
    closeConfirmationDialog,
    isConfirmationDialogOpen,
    openViewerForCursor,
    openTransferDialog,
    openCopyDialog,
    openMoveDialog,
    openDeleteDialog,
  }
}
