import { formatBytes, refreshListing } from '$lib/tauri-commands'
import { onDirectoryDiff, findFileIndex } from '$lib/tauri-commands'
import { setArchivePassword, clearArchivePassword } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { composeTransferCompleteToast } from '$lib/file-operations/transfer/transfer-complete-toast'
import { getAppLogger } from '$lib/logging/logger'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import { removeEntryFromAllSnapshots } from '$lib/search/snapshot-store.svelte'
import { pathInsideArchive, archiveNameFromPath } from './volume-capabilities'
import type { TransferDialogPropsData } from './transfer-operations'
import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
import type { TransferOperationType, SortColumn, SortOrder, ConflictResolution, WriteOperationError } from '../types'
import type { FilePaneAPI } from './types'

const log = getAppLogger('fileExplorer')

export interface TransferProgressPropsData {
  operationType: TransferOperationType
  sourcePaths: string[]
  sourceFolderPath: string
  sourcePaneSide: 'left' | 'right'
  /** Not applicable for delete/trash */
  destinationPath?: string
  /** Not applicable for delete/trash */
  direction?: 'left' | 'right'
  sortColumn: SortColumn
  sortOrder: SortOrder
  previewId: string | null
  sourceVolumeId: string
  /** Not applicable for delete/trash */
  destVolumeId?: string
  /** Not applicable for delete/trash */
  conflictResolution?: ConflictResolution
  /** Per-item sizes for trash progress (from scan or drive index) */
  itemSizes?: number[]
  /** Whether the scan preview is still running (TransferProgressDialog should subscribe to scan events) */
  scanInProgress?: boolean
  /** Source filenames known to conflict at dest (from pre-flight scan).
   *  Forwarded to the BE so it can bulk-skip them upfront under `Skip all`. */
  preKnownConflicts?: string[]
  /** Top-level files the operation will transfer (for the completion toast's per-type
   *  split). Supplied by F5/F6 (real selection counts), drag-and-drop, and clipboard
   *  paste (each from a top-level kind probe). Absent only when the split is unknown
   *  (a kind probe came back partial), where the composer falls back to file counts. */
  fileCount?: number
  /** Top-level folders the operation will transfer (for the completion toast's per-type split). */
  folderCount?: number
}

export interface NewFolderDialogPropsData {
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
}

export interface NewFileDialogPropsData {
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
}

export interface AlertDialogPropsData {
  title: string
  message: string
}

export interface TransferErrorPropsData {
  operationType: TransferOperationType
  error: WriteOperationError
}

export interface ArchivePasswordPropsData {
  /** Display name of the archive being unlocked (e.g. "photos.zip"). */
  archiveName: string
  /** True when the stored password was rejected: re-prompt with distinct copy. */
  wrongAttempt: boolean
  /** Volume the archive lives on (the archive pane's parent-drive volume id). */
  parentVolumeId: string
  /** The archive path (or an inner path) to store the password against. */
  archivePath: string
  /**
   * Which flow raised the prompt:
   * - `'transfer'`: a copy/move out of an encrypted archive; on unlock it
   *   re-dispatches the same operation via `transferProgressProps`.
   * - `'browse'`: a directory listing of a header-encrypted archive; on unlock it
   *   re-lists the same directory via `retry`.
   */
  mode: 'transfer' | 'browse'
  /** Browse mode only: re-load the same directory after the password is stored. */
  retry?: () => void
}

export interface DeleteDialogPropsData {
  sourceItems: DeleteSourceItem[]
  sourcePaths: string[]
  sourceFolderPath: string
  isPermanent: boolean
  supportsTrash: boolean
  isFromCursor: boolean
  sortColumn: SortColumn
  sortOrder: SortOrder
  sourceVolumeId: string
  /**
   * Source is INSIDE a zip. Deleting an archive entry is permanent (there's no
   * Trash inside a zip), so the dialog forces permanent mode and shows an
   * archive-specific warning instead of the generic no-trash banner.
   */
  isArchive?: boolean
  /** When true, dialog auto-confirms without user interaction (MCP auto-confirm). */
  autoConfirm?: boolean
}

export interface DialogStateDeps {
  getLeftPaneRef: () => FilePaneAPI | undefined
  getRightPaneRef: () => FilePaneAPI | undefined
  getFocusedPaneRef: () => FilePaneAPI | undefined
  getFocusedPaneSide: () => 'left' | 'right'
  getShowHiddenFiles: () => boolean
  onRefocus: () => void
  onOpenInEditor: (path: string) => void
}

/** Human-readable label for a transfer op, used in log lines. */
function transferOpLabel(op: TransferOperationType): string {
  return op === 'copy'
    ? 'Copy'
    : op === 'move'
      ? 'Move'
      : op === 'compress'
        ? 'Compress'
        : op === 'trash'
          ? 'Trash'
          : 'Delete'
}

/** Force a backend re-read on a pane's listing so file diffs are emitted promptly. */
function refreshPaneListing(paneRef: FilePaneAPI | undefined): void {
  const listingId = paneRef?.getListingId()
  if (listingId) void refreshListing(listingId)
}

export function createDialogState(deps: DialogStateDeps) {
  // Transfer dialog state (copy/move)
  let showTransferDialog = $state(false)
  let transferDialogProps = $state<TransferDialogPropsData | null>(null)

  // Transfer progress dialog state
  let showTransferProgressDialog = $state(false)
  let transferProgressProps = $state<TransferProgressPropsData | null>(null)

  // New folder dialog state
  let showNewFolderDialog = $state(false)
  let newFolderDialogProps = $state<NewFolderDialogPropsData | null>(null)

  // New file dialog state
  let showNewFileDialog = $state(false)
  let newFileDialogProps = $state<NewFileDialogPropsData | null>(null)

  // Alert dialog state
  let showAlertDialog = $state(false)
  let alertDialogProps = $state<AlertDialogPropsData | null>(null)

  // Transfer error dialog state
  let showTransferErrorDialog = $state(false)
  let transferErrorProps = $state<TransferErrorPropsData | null>(null)

  // Archive-password prompt state. Shown instead of the generic error dialog when
  // a copy/move out of an encrypted archive needs a password. `transferProgressProps`
  // is deliberately kept alive alongside this (the progress dialog is unmounted but
  // its props stay) so a successful unlock can re-dispatch the same operation and a
  // cancel can settle it through the normal refresh/selection paths.
  let showArchivePasswordDialog = $state(false)
  let archivePasswordProps = $state<ArchivePasswordPropsData | null>(null)

  // Delete dialog state
  let showDeleteDialog = $state(false)
  let deleteDialogProps = $state<DeleteDialogPropsData | null>(null)

  function getSourcePaneRef(): FilePaneAPI | undefined {
    return transferProgressProps?.sourcePaneSide === 'left' ? deps.getLeftPaneRef() : deps.getRightPaneRef()
  }

  function clearSourcePaneSelection(): void {
    getSourcePaneRef()?.clearSelection()
  }

  function snapshotSourcePaneSelection(): void {
    void getSourcePaneRef()?.snapshotSelectionForOperation()
  }

  /** Adjusts source pane selection after a cancelled operation based on the snapshot state. */
  function adjustSelectionAfterCancel(op: TransferOperationType): void {
    const prevSnapshot = getSourcePaneRef()?.clearOperationSnapshot()
    if (prevSnapshot === 'all' && op !== 'copy' && op !== 'compress') {
      // Re-select all survivors (move/delete/trash changed the source listing;
      // copy and compress leave the source listing intact, so indices still hold)
      getSourcePaneRef()?.selectAll()
    } else if (prevSnapshot == null) {
      // No snapshot taken; fall back to milestone 1 behavior
      clearSourcePaneSelection()
    }
    // For 'all' + copy: source listing unchanged, existing indices still valid
    // For array snapshot: selection already reflects survivors from diff-driven adjustment
  }

  /** Refreshes panes after a transfer completes. For move/delete/trash, refresh both panes. */
  function refreshPanesAfterTransfer() {
    const opType = transferProgressProps?.operationType
    const isDeleteOrTrash = opType === 'delete' || opType === 'trash'

    if (isDeleteOrTrash) {
      // Delete/trash: refresh both panes (both might show the affected directory)
      refreshPaneListing(deps.getLeftPaneRef())
      refreshPaneListing(deps.getRightPaneRef())
    } else {
      const destPaneRef = transferProgressProps?.direction === 'right' ? deps.getRightPaneRef() : deps.getLeftPaneRef()
      const sourcePaneRef =
        transferProgressProps?.direction === 'right' ? deps.getLeftPaneRef() : deps.getRightPaneRef()

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
  }

  return {
    // --- Reactive getters for template binding ---
    get showTransferDialog() {
      return showTransferDialog
    },
    get transferDialogProps() {
      return transferDialogProps
    },
    get showTransferProgressDialog() {
      return showTransferProgressDialog
    },
    get transferProgressProps() {
      return transferProgressProps
    },
    get showNewFolderDialog() {
      return showNewFolderDialog
    },
    get newFolderDialogProps() {
      return newFolderDialogProps
    },
    get showNewFileDialog() {
      return showNewFileDialog
    },
    get newFileDialogProps() {
      return newFileDialogProps
    },
    get showAlertDialog() {
      return showAlertDialog
    },
    get alertDialogProps() {
      return alertDialogProps
    },
    get showTransferErrorDialog() {
      return showTransferErrorDialog
    },
    get transferErrorProps() {
      return transferErrorProps
    },
    get showArchivePasswordDialog() {
      return showArchivePasswordDialog
    },
    get archivePasswordProps() {
      return archivePasswordProps
    },
    get showDeleteDialog() {
      return showDeleteDialog
    },
    get deleteDialogProps() {
      return deleteDialogProps
    },

    // --- Methods to open dialogs (called from DualPaneExplorer) ---

    showAlert(title: string, message: string) {
      alertDialogProps = { title, message }
      showAlertDialog = true
    },

    showTransfer(props: TransferDialogPropsData) {
      transferDialogProps = props
      showTransferDialog = true
    },

    /** Opens the progress dialog directly, skipping the destination picker (used by clipboard paste). */
    startTransferProgress(props: TransferProgressPropsData) {
      transferProgressProps = props
      snapshotSourcePaneSelection()
      showTransferProgressDialog = true
    },

    showNewFolder(props: NewFolderDialogPropsData) {
      newFolderDialogProps = props
      showNewFolderDialog = true
    },

    showNewFile(props: NewFileDialogPropsData) {
      newFileDialogProps = props
      showNewFileDialog = true
    },

    showDeleteConfirmation(props: DeleteDialogPropsData) {
      deleteDialogProps = props
      showDeleteDialog = true
    },

    /** Raises the browse-time archive-password prompt: a directory listing of a
     *  header-encrypted archive failed because its metadata is encrypted, so even
     *  listing needs the password. Distinct from the transfer path (no
     *  `transferProgressProps`); on unlock `retry` re-lists the same directory. */
    showArchivePasswordForBrowse(info: {
      volumeId: string
      archivePath: string
      wrongAttempt: boolean
      retry: () => void
    }) {
      log.info('Directory listing needs an archive password ({state}): {path}', {
        state: info.wrongAttempt ? 'rejected' : 'first prompt',
        path: info.archivePath,
      })
      archivePasswordProps = {
        archiveName: archiveNameFromPath(info.archivePath),
        wrongAttempt: info.wrongAttempt,
        parentVolumeId: info.volumeId,
        archivePath: info.archivePath,
        mode: 'browse',
        retry: info.retry,
      }
      showArchivePasswordDialog = true
    },

    // --- Handler functions (passed to DialogManager) ---

    handleTransferConfirm(
      destination: string,
      volumeId: string,
      previewId: string | null,
      conflictResolution: ConflictResolution,
      operationType: TransferOperationType,
      scanInProgress: boolean,
      preKnownConflicts: string[],
    ) {
      if (!transferDialogProps) return

      transferProgressProps = {
        operationType,
        sourcePaths: transferDialogProps.sourcePaths,
        sourceFolderPath: transferDialogProps.sourceFolderPath,
        sourcePaneSide: transferDialogProps.direction === 'right' ? 'left' : 'right',
        destinationPath: destination,
        direction: transferDialogProps.direction,
        sortColumn: transferDialogProps.sortColumn,
        sortOrder: transferDialogProps.sortOrder,
        previewId,
        sourceVolumeId: transferDialogProps.sourceVolumeId,
        destVolumeId: volumeId,
        conflictResolution,
        scanInProgress,
        preKnownConflicts,
        fileCount: transferDialogProps.fileCount,
        folderCount: transferDialogProps.folderCount,
      }
      snapshotSourcePaneSelection()

      showTransferDialog = false
      showTransferProgressDialog = true
      // Defer nulling props so onDestroy fires first (avoids reactive reads of nulled props)
      queueMicrotask(() => {
        transferDialogProps = null
      })
    },

    handleTransferCancel() {
      showTransferDialog = false
      transferDialogProps = null
      deps.onRefocus()
    },

    handleDeleteConfirm(previewId: string | null, isPermanent: boolean) {
      if (!deleteDialogProps) return

      const opType: TransferOperationType = isPermanent ? 'delete' : 'trash'

      // Collect per-item sizes for trash progress if available.
      // Group A wire-format: IPC sends `null` for absent sizes, so reject both null and undefined.
      const sizes = deleteDialogProps.sourceItems
        .map((item) => (item.isDirectory ? item.recursiveSize : item.size))
        .filter((s): s is number => s != null)
      const itemSizes = sizes.length === deleteDialogProps.sourceItems.length ? sizes : undefined

      transferProgressProps = {
        operationType: opType,
        sourcePaths: deleteDialogProps.sourcePaths,
        sourceFolderPath: deleteDialogProps.sourceFolderPath,
        sourcePaneSide: deps.getFocusedPaneSide(),
        sortColumn: deleteDialogProps.sortColumn,
        sortOrder: deleteDialogProps.sortOrder,
        previewId,
        sourceVolumeId: deleteDialogProps.sourceVolumeId,
        itemSizes,
      }
      snapshotSourcePaneSelection()

      showDeleteDialog = false
      deleteDialogProps = null
      showTransferProgressDialog = true
    },

    handleDeleteCancel() {
      showDeleteDialog = false
      deleteDialogProps = null
      deps.onRefocus()
    },

    handleTransferComplete(filesProcessed: number, filesSkipped: number, bytesProcessed: number) {
      const props = transferProgressProps
      const op = props?.operationType ?? 'copy'
      const opLabel = transferOpLabel(op)

      // Cross-snapshot delete sync (M8c, plan §3.7): when files are removed from
      // disk via Delete or Trash (or moved away via Move — the source path no
      // longer resolves), purge each source path from every stored
      // search-results snapshot. This is the one and only authority on the
      // "the row disappears from this snapshot AND from any other snapshot
      // containing it" rule. The snapshot store bumps its mutation tick so
      // `SearchResultsView`'s `$derived` re-evaluates and the row vanishes
      // without a manual refresh. No-op when no snapshot contains the path.
      if ((op === 'delete' || op === 'trash' || op === 'move') && props?.sourcePaths) {
        for (const sourcePath of props.sourcePaths) {
          removeEntryFromAllSnapshots(sourcePath)
        }
      }
      log.info(
        `${opLabel} complete: ${String(filesProcessed)} files (${String(filesSkipped)} skipped, ${formatBytes(bytesProcessed)})`,
      )
      // Top-level counts for the per-type split ("Moved 1 file and 3 folders").
      // F5/F6, drag-and-drop, and clipboard paste all supply these now; absent
      // only when a kind probe came back partial → composer falls back.
      const toastMessage = composeTransferCompleteToast({
        operationType: op,
        filesProcessed,
        filesSkipped,
        fileCount: props?.fileCount,
        folderCount: props?.folderCount,
      })
      // `info` for the all-skipped case (nothing actually moved/copied — neutral
      // outcome, not a success). `success` everywhere else, including mixed: the
      // user's intent landed at the target.
      const allSkipped = filesSkipped > 0 && filesSkipped === filesProcessed
      // Bump the timeout for the long mixed/all-skipped sentences (default 4s reads as
      // a flicker for users still parsing the second clause). 7s comfortably covers the
      // longest variant without staying around long enough to nag.
      addToast(toastMessage, { level: allSkipped ? 'info' : 'success', timeoutMs: 7000 })

      refreshPanesAfterTransfer()
      getSourcePaneRef()?.clearOperationSnapshot()
      clearSourcePaneSelection()

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    /** The operation was sent to the background (Queue button / F2 / auto-queue):
     *  keep it running, just close its modal. The op is now managed in the queue
     *  window. We do NOT cancel it and do NOT refresh panes here — the op is still
     *  in flight; the file watcher and the queue window cover its lifecycle. We DO
     *  drop the source-pane operation snapshot and selection, since this dialog
     *  has handed the op off and won't fire `handleTransferComplete` for it. */
    handleTransferQueue() {
      const op = transferProgressProps?.operationType ?? 'copy'
      log.info('{op} sent to the background (managed in the queue window)', { op: transferOpLabel(op) })

      getSourcePaneRef()?.clearOperationSnapshot()
      clearSourcePaneSelection()

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferCancelled(filesProcessed: number) {
      const op = transferProgressProps?.operationType ?? 'copy'
      const opLabel = transferOpLabel(op)
      log.info(`${opLabel} cancelled after ${String(filesProcessed)} files`)

      refreshPanesAfterTransfer()
      adjustSelectionAfterCancel(op)

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferError(error: WriteOperationError) {
      const op = transferProgressProps?.operationType ?? 'copy'
      const opLabel = transferOpLabel(op)

      // An encrypted-archive source needs a password: intercept BEFORE the generic
      // error dialog and prompt instead. Keep `transferProgressProps` alive (only
      // unmount the progress dialog) so an unlock can re-dispatch the same op and a
      // cancel can settle through the normal refresh/selection paths. The parent
      // volume the archive lives on is the source pane's volume id (an archive pane
      // keeps its parent drive's id); the archive path is the errored source path.
      if (error.type === 'archive_needs_password' && transferProgressProps) {
        log.info('{op} operation needs an archive password ({state}): {path}', {
          op: opLabel,
          state: error.wrongAttempt ? 'rejected' : 'first prompt',
          path: error.path,
        })
        archivePasswordProps = {
          archiveName: archiveNameFromPath(error.path),
          wrongAttempt: error.wrongAttempt,
          parentVolumeId: transferProgressProps.sourceVolumeId,
          archivePath: error.path,
          mode: 'transfer',
        }
        showTransferProgressDialog = false
        showArchivePasswordDialog = true
        return
      }

      log.error('{op} failed: {errorType}', {
        op: opLabel,
        errorType: error.type,
        error,
      })

      refreshPanesAfterTransfer()
      getSourcePaneRef()?.clearOperationSnapshot()
      clearSourcePaneSelection()

      showTransferProgressDialog = false
      transferProgressProps = null

      transferErrorProps = { operationType: op, error }
      showTransferErrorDialog = true
    },

    /** Stores the entered password on the backend, then retries whatever raised
     *  the prompt: browse mode re-lists the directory, transfer mode re-dispatches
     *  the copy/move. Either way a wrong password re-raises the prompt (with
     *  `wrongAttempt: true`). */
    handleArchivePasswordSubmit(password: string) {
      const pw = archivePasswordProps
      if (!pw) return

      // Browse path: store the password, then re-list the SAME directory. A wrong
      // password makes the re-list raise `archiveNeedsPassword` with
      // `wrongAttempt: true`, so the loader re-invokes the browse prompt.
      if (pw.mode === 'browse') {
        const retry = pw.retry
        showArchivePasswordDialog = false
        archivePasswordProps = null
        void (async () => {
          try {
            await setArchivePassword(pw.parentVolumeId, pw.archivePath, password)
          } catch (err) {
            // A store failure means the re-list will re-prompt (the password never
            // landed); surface nothing beyond the log.
            log.warn('Failed to store archive password: {error}', { error: err })
          }
          retry?.()
        })()
        return
      }

      // Transfer path: store the password, then re-dispatch the same copy/move so
      // the extract path can decrypt. A fresh scan runs (the previous preview was
      // consumed), so `previewId` is cleared and `scanInProgress` off. A wrong
      // password again raises `archive_needs_password` with `wrongAttempt: true`.
      const props = transferProgressProps
      if (!props) return

      showArchivePasswordDialog = false
      archivePasswordProps = null

      void (async () => {
        try {
          await setArchivePassword(pw.parentVolumeId, pw.archivePath, password)
        } catch (err) {
          // A store failure just means the retry will re-prompt (the password
          // never landed); surface nothing beyond the log.
          log.warn('Failed to store archive password: {error}', { error: err })
        }
        transferProgressProps = { ...props, previewId: null, scanInProgress: false }
        snapshotSourcePaneSelection()
        showTransferProgressDialog = true
      })()
    },

    /** The user dismissed the password prompt: forget any stored password. Browse
     *  mode just closes the prompt, leaving the "This archive needs a password"
     *  fallback pane in place (the loader already settled it), so the user simply
     *  doesn't get in. Transfer mode settles the operation exactly as a dismissed
     *  transfer error would (refresh panes, drop the source-pane snapshot and
     *  selection), so nothing looks stuck. */
    handleArchivePasswordCancel() {
      const pw = archivePasswordProps
      if (pw) {
        void clearArchivePassword(pw.parentVolumeId, pw.archivePath)
      }

      if (pw?.mode === 'browse') {
        log.info('Browse archive-password prompt cancelled')
        showArchivePasswordDialog = false
        archivePasswordProps = null
        deps.onRefocus()
        return
      }

      const op = transferProgressProps?.operationType ?? 'copy'
      log.info('{op} archive-password prompt cancelled', { op: transferOpLabel(op) })

      refreshPanesAfterTransfer()
      getSourcePaneRef()?.clearOperationSnapshot()
      clearSourcePaneSelection()

      showArchivePasswordDialog = false
      archivePasswordProps = null
      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferErrorClose() {
      showTransferErrorDialog = false
      transferErrorProps = null
      deps.onRefocus()
    },

    handleNewFolderCreated(folderName: string) {
      const paneRef = deps.getFocusedPaneRef()
      const paneListingId = paneRef?.getListingId()
      const hasParent = paneRef?.hasParentEntry()

      showNewFolderDialog = false
      newFolderDialogProps = null
      deps.onRefocus()

      if (!paneListingId) return
      void moveCursorToNewFolder(
        paneListingId,
        folderName,
        paneRef,
        hasParent ?? false,
        deps.getShowHiddenFiles(),
        onDirectoryDiff,
        findFileIndex,
      )
    },

    handleNewFolderCancel() {
      showNewFolderDialog = false
      newFolderDialogProps = null
      deps.onRefocus()
    },

    handleNewFileCreated(fileName: string) {
      const paneRef = deps.getFocusedPaneRef()
      const paneListingId = paneRef?.getListingId()
      const hasParent = paneRef?.hasParentEntry()
      const currentPath = newFileDialogProps?.currentPath ?? ''

      showNewFileDialog = false
      newFileDialogProps = null
      deps.onRefocus()

      if (paneListingId) {
        void moveCursorToNewFolder(
          paneListingId,
          fileName,
          paneRef,
          hasParent ?? false,
          deps.getShowHiddenFiles(),
          onDirectoryDiff,
          findFileIndex,
        )
      }

      // Open the newly created file in the default editor — but NOT for a file
      // created inside a zip: that's an async managed archive-edit, so the entry
      // doesn't exist yet when this runs (the create returns an op id, not a
      // landed path), and an archive-inner path isn't openable in an external
      // editor anyway. The cursor still lands on it after the edit's refresh.
      if (pathInsideArchive(currentPath)) return
      const fullPath = currentPath === '/' ? `/${fileName}` : `${currentPath}/${fileName}`
      deps.onOpenInEditor(fullPath)
    },

    handleNewFileCancel() {
      showNewFileDialog = false
      newFileDialogProps = null
      deps.onRefocus()
    },

    handleAlertClose() {
      showAlertDialog = false
      alertDialogProps = null
      deps.onRefocus()
    },

    // --- Query methods ---

    /** Closes any confirmation dialog (new folder, new file, transfer, or delete) if open (for MCP). */
    closeConfirmationDialog() {
      if (showNewFolderDialog) {
        showNewFolderDialog = false
        newFolderDialogProps = null
        deps.onRefocus()
      }
      if (showNewFileDialog) {
        showNewFileDialog = false
        newFileDialogProps = null
        deps.onRefocus()
      }
      if (showTransferDialog) {
        showTransferDialog = false
        transferDialogProps = null
        deps.onRefocus()
      }
      if (showDeleteDialog) {
        showDeleteDialog = false
        deleteDialogProps = null
        deps.onRefocus()
      }
    },

    isConfirmationDialogOpen(): boolean {
      return showNewFolderDialog || showNewFileDialog || showTransferDialog || showDeleteDialog
    },

    /** Whether any transfer/delete-related dialog is open (used by canSwapPanes). */
    isAnyTransferDialogOpen(): boolean {
      return showTransferDialog || showTransferProgressDialog || showDeleteDialog
    },

    /** Programmatically confirm an open dialog (for MCP confirm action). */
    confirmOpenDialog(dialogType: string, onConflict?: string) {
      if (dialogType === 'transfer-confirmation' && showTransferDialog && transferDialogProps) {
        // Map onConflict to ConflictResolution
        const conflictMap: Record<string, ConflictResolution> = {
          skip_all: 'skip',
          overwrite_all: 'overwrite',
          rename_all: 'rename',
        }
        const resolution: ConflictResolution = (onConflict ? conflictMap[onConflict] : undefined) ?? 'skip'
        this.handleTransferConfirm(
          transferDialogProps.destinationPath,
          transferDialogProps.destVolumeId,
          null, // previewId not available when confirming programmatically
          resolution,
          transferDialogProps.operationType,
          false, // scanInProgress not tracked when confirming programmatically
          [], // pre-known conflicts not available when confirming programmatically
        )
      } else if (dialogType === 'delete-confirmation' && showDeleteDialog && deleteDialogProps) {
        // previewId not available when confirming programmatically.
        // For MCP auto-confirm, honor whatever the props initialized with.
        const isPermanent = deleteDialogProps.isPermanent || !deleteDialogProps.supportsTrash
        this.handleDeleteConfirm(null, isPermanent)
      }
    },
  }
}
