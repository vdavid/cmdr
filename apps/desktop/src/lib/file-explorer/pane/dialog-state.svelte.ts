import { formatBytes, refreshListing } from '$lib/tauri-commands'
import { listen, findFileIndex } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
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
      // No snapshot taken — fall back to milestone 1 behavior
      clearSourcePaneSelection()
    }
    // For 'all' + copy: source listing unchanged, existing indices still valid
    // For array snapshot: selection already reflects survivors from diff-driven adjustment
  }

  /** Refreshes panes after a transfer completes — for move/delete/trash, refresh both panes. */
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

    // Refresh disk space on both panes — both might be on the same volume
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
      _volumeId: string,
      previewId: string | null,
      conflictResolution: ConflictResolution,
      operationType: TransferOperationType,
      scanInProgress: boolean,
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
        destVolumeId: transferDialogProps.destVolumeId,
        conflictResolution,
        scanInProgress,
      }
      snapshotSourcePaneSelection()

      showTransferDialog = false
      transferDialogProps = null
      showTransferProgressDialog = true
    },

    handleTransferCancel() {
      showTransferDialog = false
      transferDialogProps = null
      deps.onRefocus()
    },

    handleDeleteConfirm(previewId: string | null) {
      if (!deleteDialogProps) return

      const isPermanent = deleteDialogProps.isPermanent || !deleteDialogProps.supportsTrash
      const opType: TransferOperationType = isPermanent ? 'delete' : 'trash'

      // Collect per-item sizes for trash progress if available
      const sizes = deleteDialogProps.sourceItems
        .map((item) => (item.isDirectory ? item.recursiveSize : item.size))
        .filter((s): s is number => s !== undefined)
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

    handleTransferComplete(filesProcessed: number, bytesProcessed: number) {
      const op = transferProgressProps?.operationType ?? 'copy'
      const opLabel = op === 'copy' ? 'Copy' : op === 'move' ? 'Move' : op === 'trash' ? 'Trash' : 'Delete'
      log.info(`${opLabel} complete: ${String(filesProcessed)} files (${formatBytes(bytesProcessed)})`)
      const itemWord = filesProcessed === 1 ? 'file' : 'files'
      const toastMessage =
        op === 'trash'
          ? `Moved ${String(filesProcessed)} ${itemWord} to trash`
          : `${opLabel} complete: ${String(filesProcessed)} ${itemWord}`
      addToast(toastMessage)

      refreshPanesAfterTransfer()
      getSourcePaneRef()?.clearOperationSnapshot()
      clearSourcePaneSelection()

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferCancelled(filesProcessed: number) {
      const op = transferProgressProps?.operationType ?? 'copy'
      const opLabel = op === 'copy' ? 'Copy' : op === 'move' ? 'Move' : op === 'trash' ? 'Trash' : 'Delete'
      log.info(`${opLabel} cancelled after ${String(filesProcessed)} files`)

      refreshPanesAfterTransfer()
      adjustSelectionAfterCancel(op)

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferError(error: WriteOperationError) {
      const op = transferProgressProps?.operationType ?? 'copy'
      const opLabel = op === 'copy' ? 'Copy' : op === 'move' ? 'Move' : op === 'trash' ? 'Trash' : 'Delete'
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
        const resolution: ConflictResolution = (onConflict && conflictMap[onConflict]) || 'skip'
        this.handleTransferConfirm(
          transferDialogProps.destinationPath,
          transferDialogProps.destVolumeId,
          null, // previewId not available when confirming programmatically
          resolution,
          transferDialogProps.operationType,
          false, // scanInProgress not tracked when confirming programmatically
        )
      } else if (dialogType === 'delete-confirmation' && showDeleteDialog) {
        this.handleDeleteConfirm(null) // previewId not available
      }
    },
  }
}
