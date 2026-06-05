import { formatBytes, refreshListing } from '$lib/tauri-commands'
import { listen, findFileIndex } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { composeTransferCompleteToast } from '$lib/file-operations/transfer/transfer-complete-toast'
import { getAppLogger } from '$lib/logging/logger'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import { removeEntryFromAllSnapshots } from '$lib/search/snapshot-store.svelte'
import type { TransferDialogPropsData } from './transfer-operations'
import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
import type {
  TransferOperationType,
  SortColumn,
  SortOrder,
  ConflictResolution,
  WriteOperationError,
  FriendlyError,
} from '../types'
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
  /** Top-level files the user selected (for the completion toast's per-type split).
   *  Absent on the clipboard-paste path, where the per-type split isn't known. */
  fileCount?: number
  /** Top-level folders the user selected (for the completion toast's per-type split). */
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
  /** Backend-supplied friendly error info; preferred over the FE-derived copy when present. */
  friendly?: FriendlyError
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
  return op === 'copy' ? 'Copy' : op === 'move' ? 'Move' : op === 'trash' ? 'Trash' : 'Delete'
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
    if (prevSnapshot === 'all' && op !== 'copy') {
      // Re-select all survivors (move/delete/trash changed the source listing)
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
      // Top-level selection counts for the per-type split ("Moved 1 file and 3
      // folders"). Absent on the clipboard-paste path → composer falls back.
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

    handleTransferError(error: WriteOperationError, friendly?: FriendlyError) {
      const op = transferProgressProps?.operationType ?? 'copy'
      const opLabel = transferOpLabel(op)
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

      transferErrorProps = { operationType: op, error, friendly }
      showTransferErrorDialog = true
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
        listen,
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
          listen,
          findFileIndex,
        )
      }

      // Open the newly created file in the default editor
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
