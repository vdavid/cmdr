import { formatBytes, refreshListing } from '$lib/tauri-commands'
import { listen, findFileIndex } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import type { TransferDialogPropsData } from './transfer-operations'
import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
import type { TransferOperationType, SortColumn, SortOrder, ConflictResolution, WriteOperationError } from '../types'
import type FilePane from './FilePane.svelte'

const log = getAppLogger('fileExplorer')

export interface TransferProgressPropsData {
    operationType: TransferOperationType
    sourcePaths: string[]
    sourceFolderPath: string
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
}

export interface NewFolderDialogPropsData {
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
}

export interface DialogStateDeps {
    getLeftPaneRef: () => FilePane | undefined
    getRightPaneRef: () => FilePane | undefined
    getFocusedPaneRef: () => FilePane | undefined
    getShowHiddenFiles: () => boolean
    onRefocus: () => void
}

/** Force a backend re-read on a pane's listing so file diffs are emitted promptly. */
function refreshPaneListing(paneRef: FilePane | undefined): void {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const listingId = paneRef?.getListingId?.() as string | undefined
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

    // Alert dialog state
    let showAlertDialog = $state(false)
    let alertDialogProps = $state<AlertDialogPropsData | null>(null)

    // Transfer error dialog state
    let showTransferErrorDialog = $state(false)
    let transferErrorProps = $state<TransferErrorPropsData | null>(null)

    // Delete dialog state
    let showDeleteDialog = $state(false)
    let deleteDialogProps = $state<DeleteDialogPropsData | null>(null)

    /** Refreshes panes after a transfer completes — for move/delete/trash, refresh both panes. */
    function refreshPanesAfterTransfer() {
        const opType = transferProgressProps?.operationType
        const isDeleteOrTrash = opType === 'delete' || opType === 'trash'

        if (isDeleteOrTrash) {
            // Delete/trash: refresh both panes (both might show the affected directory)
            refreshPaneListing(deps.getLeftPaneRef())
            refreshPaneListing(deps.getRightPaneRef())
        } else {
            const destPaneRef =
                transferProgressProps?.direction === 'right' ? deps.getRightPaneRef() : deps.getLeftPaneRef()
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
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        void deps.getLeftPaneRef()?.refreshVolumeSpace?.()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        void deps.getRightPaneRef()?.refreshVolumeSpace?.()
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

        showNewFolder(props: NewFolderDialogPropsData) {
            newFolderDialogProps = props
            showNewFolderDialog = true
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
        ) {
            if (!transferDialogProps) return

            transferProgressProps = {
                operationType,
                sourcePaths: transferDialogProps.sourcePaths,
                sourceFolderPath: transferDialogProps.sourceFolderPath,
                destinationPath: destination,
                direction: transferDialogProps.direction,
                sortColumn: transferDialogProps.sortColumn,
                sortOrder: transferDialogProps.sortOrder,
                previewId,
                sourceVolumeId: transferDialogProps.sourceVolumeId,
                destVolumeId: transferDialogProps.destVolumeId,
                conflictResolution,
            }

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
                sortColumn: deleteDialogProps.sortColumn,
                sortOrder: deleteDialogProps.sortOrder,
                previewId,
                sourceVolumeId: deleteDialogProps.sourceVolumeId,
                itemSizes,
            }

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

            showTransferProgressDialog = false
            transferProgressProps = null
            deps.onRefocus()
        },

        handleTransferCancelled(filesProcessed: number) {
            const op = transferProgressProps?.operationType ?? 'copy'
            const opLabel = op === 'copy' ? 'Copy' : op === 'move' ? 'Move' : op === 'trash' ? 'Trash' : 'Delete'
            log.info(`${opLabel} cancelled after ${String(filesProcessed)} files`)

            refreshPanesAfterTransfer()

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
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            const paneListingId = paneRef?.getListingId?.() as string | undefined
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            const hasParent = paneRef?.hasParentEntry?.() as boolean | undefined

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

        handleAlertClose() {
            showAlertDialog = false
            alertDialogProps = null
            deps.onRefocus()
        },

        // --- Query methods ---

        /** Closes any confirmation dialog (new folder, transfer, or delete) if open (for MCP). */
        closeConfirmationDialog() {
            if (showNewFolderDialog) {
                showNewFolderDialog = false
                newFolderDialogProps = null
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
            return showNewFolderDialog || showTransferDialog || showDeleteDialog
        },

        /** Whether any transfer/delete-related dialog is open (used by canSwapPanes). */
        isAnyTransferDialogOpen(): boolean {
            return showTransferDialog || showTransferProgressDialog || showDeleteDialog
        },
    }
}
