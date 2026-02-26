<script lang="ts">
    import TransferDialog from '../../file-operations/transfer/TransferDialog.svelte'
    import TransferProgressDialog from '../../file-operations/transfer/TransferProgressDialog.svelte'
    import TransferErrorDialog from '../../file-operations/transfer/TransferErrorDialog.svelte'
    import NewFolderDialog from '$lib/file-operations/mkdir/NewFolderDialog.svelte'
    import AlertDialog from '$lib/ui/AlertDialog.svelte'
    import type { TransferDialogPropsData } from './transfer-operations'
    import type {
        TransferProgressPropsData,
        NewFolderDialogPropsData,
        AlertDialogPropsData,
        TransferErrorPropsData,
    } from './dialog-state.svelte'
    import type { VolumeInfo, ConflictResolution, TransferOperationType, WriteOperationError } from '../types'

    const {
        showTransferDialog,
        transferDialogProps,
        volumes,
        showTransferProgressDialog,
        transferProgressProps,
        showNewFolderDialog,
        newFolderDialogProps,
        showAlertDialog,
        alertDialogProps,
        showTransferErrorDialog,
        transferErrorProps,
        onTransferConfirm,
        onTransferCancel,
        onTransferComplete,
        onTransferCancelled,
        onTransferError,
        onTransferErrorClose,
        onNewFolderCreated,
        onNewFolderCancel,
        onAlertClose,
    }: {
        showTransferDialog: boolean
        transferDialogProps: TransferDialogPropsData | null
        volumes: VolumeInfo[]
        showTransferProgressDialog: boolean
        transferProgressProps: TransferProgressPropsData | null
        showNewFolderDialog: boolean
        newFolderDialogProps: NewFolderDialogPropsData | null
        showAlertDialog: boolean
        alertDialogProps: AlertDialogPropsData | null
        showTransferErrorDialog: boolean
        transferErrorProps: TransferErrorPropsData | null
        onTransferConfirm: (
            destination: string,
            volumeId: string,
            previewId: string | null,
            conflictResolution: ConflictResolution,
            operationType: TransferOperationType,
        ) => void
        onTransferCancel: () => void
        onTransferComplete: (filesProcessed: number, bytesProcessed: number) => void
        onTransferCancelled: (filesProcessed: number) => void
        onTransferError: (error: WriteOperationError) => void
        onTransferErrorClose: () => void
        onNewFolderCreated: (folderName: string) => void
        onNewFolderCancel: () => void
        onAlertClose: () => void
    } = $props()
</script>

{#if showTransferDialog && transferDialogProps}
    {#key transferDialogProps}
        <TransferDialog
            operationType={transferDialogProps.operationType}
            sourcePaths={transferDialogProps.sourcePaths}
            destinationPath={transferDialogProps.destinationPath}
            direction={transferDialogProps.direction}
            {volumes}
            currentVolumeId={transferDialogProps.currentVolumeId}
            fileCount={transferDialogProps.fileCount}
            folderCount={transferDialogProps.folderCount}
            sourceFolderPath={transferDialogProps.sourceFolderPath}
            sortColumn={transferDialogProps.sortColumn}
            sortOrder={transferDialogProps.sortOrder}
            sourceVolumeId={transferDialogProps.sourceVolumeId}
            destVolumeId={transferDialogProps.destVolumeId}
            allowOperationToggle={transferDialogProps.allowOperationToggle}
            onConfirm={onTransferConfirm}
            onCancel={onTransferCancel}
        />
    {/key}
{/if}

{#if showTransferProgressDialog && transferProgressProps}
    <TransferProgressDialog
        operationType={transferProgressProps.operationType}
        sourcePaths={transferProgressProps.sourcePaths}
        sourceFolderPath={transferProgressProps.sourceFolderPath}
        destinationPath={transferProgressProps.destinationPath}
        direction={transferProgressProps.direction}
        sortColumn={transferProgressProps.sortColumn}
        sortOrder={transferProgressProps.sortOrder}
        previewId={transferProgressProps.previewId}
        sourceVolumeId={transferProgressProps.sourceVolumeId}
        destVolumeId={transferProgressProps.destVolumeId}
        conflictResolution={transferProgressProps.conflictResolution}
        onComplete={onTransferComplete}
        onCancelled={onTransferCancelled}
        onError={onTransferError}
    />
{/if}

{#if showNewFolderDialog && newFolderDialogProps}
    <NewFolderDialog
        currentPath={newFolderDialogProps.currentPath}
        listingId={newFolderDialogProps.listingId}
        showHiddenFiles={newFolderDialogProps.showHiddenFiles}
        initialName={newFolderDialogProps.initialName}
        volumeId={newFolderDialogProps.volumeId}
        onCreated={onNewFolderCreated}
        onCancel={onNewFolderCancel}
    />
{/if}

{#if showAlertDialog && alertDialogProps}
    <AlertDialog title={alertDialogProps.title} message={alertDialogProps.message} onClose={onAlertClose} />
{/if}

{#if showTransferErrorDialog && transferErrorProps}
    <TransferErrorDialog
        operationType={transferErrorProps.operationType}
        error={transferErrorProps.error}
        onClose={onTransferErrorClose}
    />
{/if}
