import { formatBytes, refreshListing } from '$lib/tauri-commands'
import { listen, findFileIndex } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import type { TransferDialogPropsData } from './transfer-operations'
import type { TransferOperationType, SortColumn, SortOrder, ConflictResolution, WriteOperationError } from '../types'
import type FilePane from './FilePane.svelte'

const log = getAppLogger('fileExplorer')

export interface TransferProgressPropsData {
    operationType: TransferOperationType
    sourcePaths: string[]
    sourceFolderPath: string
    destinationPath: string
    direction: 'left' | 'right'
    sortColumn: SortColumn
    sortOrder: SortOrder
    previewId: string | null
    sourceVolumeId: string
    destVolumeId: string
    conflictResolution: ConflictResolution
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

    /** Refreshes panes after a transfer completes — for move, refresh both panes. */
    function refreshPanesAfterTransfer() {
        const destPaneRef =
            transferProgressProps?.direction === 'right' ? deps.getRightPaneRef() : deps.getLeftPaneRef()
        const sourcePaneRef =
            transferProgressProps?.direction === 'right' ? deps.getLeftPaneRef() : deps.getRightPaneRef()

        // Force backend to re-read directories and emit diffs. The file watcher may
        // not have fired yet (common for instant renames on Linux), leaving stale cache.
        refreshPaneListing(destPaneRef)
        if (transferProgressProps?.operationType === 'move') {
            refreshPaneListing(sourcePaneRef)
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

        handleTransferComplete(filesProcessed: number, bytesProcessed: number) {
            const op = transferProgressProps?.operationType ?? 'copy'
            log.info(
                `${op === 'copy' ? 'Copy' : 'Move'} complete: ${String(filesProcessed)} files (${formatBytes(bytesProcessed)})`,
            )
            addToast(
                `${op === 'copy' ? 'Copy' : 'Move'} complete: ${String(filesProcessed)} ${filesProcessed === 1 ? 'file' : 'files'}`,
            )

            refreshPanesAfterTransfer()

            showTransferProgressDialog = false
            transferProgressProps = null
            deps.onRefocus()
        },

        handleTransferCancelled(filesProcessed: number) {
            const op = transferProgressProps?.operationType ?? 'copy'
            log.info(`${op === 'copy' ? 'Copy' : 'Move'} cancelled after ${String(filesProcessed)} files`)

            refreshPanesAfterTransfer()

            showTransferProgressDialog = false
            transferProgressProps = null
            deps.onRefocus()
        },

        handleTransferError(error: WriteOperationError) {
            const op = transferProgressProps?.operationType ?? 'copy'
            log.error('{op} failed: {errorType}', {
                op: op === 'copy' ? 'Copy' : 'Move',
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

        /** Closes any confirmation dialog (new folder or transfer) if open (for MCP). */
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
        },

        isConfirmationDialogOpen(): boolean {
            return showNewFolderDialog || showTransferDialog
        },

        /** Whether any transfer-related dialog is open (used by canSwapPanes). */
        isAnyTransferDialogOpen(): boolean {
            return showTransferDialog || showTransferProgressDialog
        },
    }
}
