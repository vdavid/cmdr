<script lang="ts">
    import TransferDialog from '../../file-operations/transfer/TransferDialog.svelte'
    import TransferProgressDialog from '../../file-operations/transfer/TransferProgressDialog.svelte'
    import TransferErrorDialog from '../../file-operations/transfer/TransferErrorDialog.svelte'
    import ArchivePasswordDialog from '../../file-operations/transfer/ArchivePasswordDialog.svelte'
    import DeleteDialog from '$lib/file-operations/delete/DeleteDialog.svelte'
    import NewFolderDialog from '$lib/file-operations/mkdir/NewFolderDialog.svelte'
    import NewFileDialog from '$lib/file-operations/mkfile/NewFileDialog.svelte'
    import AlertDialog from '$lib/ui/AlertDialog.svelte'
    import type { TransferDialogPropsData } from './transfer-operations'
    import type {
        TransferProgressPropsData,
        NewFolderDialogPropsData,
        NewFileDialogPropsData,
        AlertDialogPropsData,
        TransferErrorPropsData,
        ArchivePasswordPropsData,
        DeleteDialogPropsData,
    } from './dialog-state.svelte'
    import type { ConflictResolution, FriendlyError, TransferOperationType, WriteOperationError } from '../types'

    const {
        showTransferDialog,
        transferDialogProps,
        showTransferProgressDialog,
        transferProgressProps,
        showNewFolderDialog,
        newFolderDialogProps,
        showNewFileDialog,
        newFileDialogProps,
        showAlertDialog,
        alertDialogProps,
        showTransferErrorDialog,
        transferErrorProps,
        showArchivePasswordDialog,
        archivePasswordProps,
        showDeleteDialog,
        deleteDialogProps,
        onTransferConfirm,
        onTransferCancel,
        onTransferComplete,
        onTransferCancelled,
        onTransferError,
        onTransferQueue,
        onTransferErrorClose,
        onArchivePasswordSubmit,
        onArchivePasswordCancel,
        onNewFolderCreated,
        onNewFolderCancel,
        onNewFileCreated,
        onNewFileCancel,
        onAlertClose,
        onDeleteConfirm,
        onDeleteCancel,
    }: {
        showTransferDialog: boolean
        transferDialogProps: TransferDialogPropsData | null
        showTransferProgressDialog: boolean
        transferProgressProps: TransferProgressPropsData | null
        showNewFolderDialog: boolean
        newFolderDialogProps: NewFolderDialogPropsData | null
        showNewFileDialog: boolean
        newFileDialogProps: NewFileDialogPropsData | null
        showAlertDialog: boolean
        alertDialogProps: AlertDialogPropsData | null
        showTransferErrorDialog: boolean
        transferErrorProps: TransferErrorPropsData | null
        showArchivePasswordDialog: boolean
        archivePasswordProps: ArchivePasswordPropsData | null
        showDeleteDialog: boolean
        deleteDialogProps: DeleteDialogPropsData | null
        onTransferConfirm: (
            destination: string,
            volumeId: string,
            previewId: string | null,
            conflictResolution: ConflictResolution,
            operationType: TransferOperationType,
            scanInProgress: boolean,
            preKnownConflicts: string[],
        ) => void
        onTransferCancel: () => void
        onTransferComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
        onTransferCancelled: (filesProcessed: number) => void
        onTransferError: (error: WriteOperationError, friendly?: FriendlyError) => void
        onTransferQueue: () => void
        onTransferErrorClose: () => void
        onArchivePasswordSubmit: (password: string) => void
        onArchivePasswordCancel: () => void
        onNewFolderCreated: (folderName: string) => void
        onNewFolderCancel: () => void
        onNewFileCreated: (fileName: string) => void
        onNewFileCancel: () => void
        onAlertClose: () => void
        onDeleteConfirm: (previewId: string | null, isPermanent: boolean) => void
        onDeleteCancel: () => void
    } = $props()
</script>

{#if showTransferDialog && transferDialogProps}
    {#key transferDialogProps}
        <TransferDialog
            operationType={transferDialogProps.operationType}
            sourcePaths={transferDialogProps.sourcePaths}
            destinationPath={transferDialogProps.destinationPath}
            direction={transferDialogProps.direction}
            currentVolumeId={transferDialogProps.currentVolumeId}
            fileCount={transferDialogProps.fileCount}
            folderCount={transferDialogProps.folderCount}
            sourceFolderPath={transferDialogProps.sourceFolderPath}
            sortColumn={transferDialogProps.sortColumn}
            sortOrder={transferDialogProps.sortOrder}
            sourceVolumeId={transferDialogProps.sourceVolumeId}
            destVolumeId={transferDialogProps.destVolumeId}
            autoConfirm={transferDialogProps.autoConfirm}
            autoConfirmOnConflict={transferDialogProps.autoConfirmOnConflict}
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
        preKnownConflicts={transferProgressProps.preKnownConflicts}
        itemSizes={transferProgressProps.itemSizes}
        scanInProgress={transferProgressProps.scanInProgress}
        onComplete={onTransferComplete}
        onCancelled={onTransferCancelled}
        onError={onTransferError}
        onQueue={onTransferQueue}
    />
{/if}

{#if showDeleteDialog && deleteDialogProps}
    {#key deleteDialogProps}
        <DeleteDialog
            sourceItems={deleteDialogProps.sourceItems}
            sourcePaths={deleteDialogProps.sourcePaths}
            sourceFolderPath={deleteDialogProps.sourceFolderPath}
            isPermanent={deleteDialogProps.isPermanent}
            supportsTrash={deleteDialogProps.supportsTrash}
            isArchive={deleteDialogProps.isArchive}
            isFromCursor={deleteDialogProps.isFromCursor}
            sortColumn={deleteDialogProps.sortColumn}
            sortOrder={deleteDialogProps.sortOrder}
            sourceVolumeId={deleteDialogProps.sourceVolumeId}
            autoConfirm={deleteDialogProps.autoConfirm}
            onConfirm={onDeleteConfirm}
            onCancel={onDeleteCancel}
        />
    {/key}
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

{#if showNewFileDialog && newFileDialogProps}
    <NewFileDialog
        currentPath={newFileDialogProps.currentPath}
        listingId={newFileDialogProps.listingId}
        showHiddenFiles={newFileDialogProps.showHiddenFiles}
        initialName={newFileDialogProps.initialName}
        volumeId={newFileDialogProps.volumeId}
        onCreated={onNewFileCreated}
        onCancel={onNewFileCancel}
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

{#if showArchivePasswordDialog && archivePasswordProps}
    <ArchivePasswordDialog
        archiveName={archivePasswordProps.archiveName}
        wrongAttempt={archivePasswordProps.wrongAttempt}
        onSubmit={onArchivePasswordSubmit}
        onCancel={onArchivePasswordCancel}
    />
{/if}
