<script lang="ts">
    import CopyDialog from '../write-operations/CopyDialog.svelte'
    import CopyProgressDialog from '../write-operations/CopyProgressDialog.svelte'
    import CopyErrorDialog from '../write-operations/CopyErrorDialog.svelte'
    import NewFolderDialog from './NewFolderDialog.svelte'
    import AlertDialog from '$lib/AlertDialog.svelte'
    import type { CopyDialogPropsData } from './copy-operations'
    import type { SortColumn, SortOrder, VolumeInfo, ConflictResolution, WriteOperationError } from './types'

    const {
        showCopyDialog,
        copyDialogProps,
        volumes,
        showCopyProgressDialog,
        copyProgressProps,
        showNewFolderDialog,
        newFolderDialogProps,
        showAlertDialog,
        alertDialogProps,
        showCopyErrorDialog,
        copyErrorProps,
        onCopyConfirm,
        onCopyCancel,
        onCopyComplete,
        onCopyCancelled,
        onCopyError,
        onCopyErrorClose,
        onNewFolderCreated,
        onNewFolderCancel,
        onAlertClose,
    }: {
        showCopyDialog: boolean
        copyDialogProps: CopyDialogPropsData | null
        volumes: VolumeInfo[]
        showCopyProgressDialog: boolean
        copyProgressProps: {
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
        } | null
        showNewFolderDialog: boolean
        newFolderDialogProps: {
            currentPath: string
            listingId: string
            showHiddenFiles: boolean
            initialName: string
            volumeId: string
        } | null
        showAlertDialog: boolean
        alertDialogProps: { title: string; message: string } | null
        showCopyErrorDialog: boolean
        copyErrorProps: { error: WriteOperationError } | null
        onCopyConfirm: (
            destination: string,
            volumeId: string,
            previewId: string | null,
            conflictResolution: ConflictResolution,
        ) => void
        onCopyCancel: () => void
        onCopyComplete: (filesProcessed: number, bytesProcessed: number) => void
        onCopyCancelled: (filesProcessed: number) => void
        onCopyError: (error: WriteOperationError) => void
        onCopyErrorClose: () => void
        onNewFolderCreated: (folderName: string) => void
        onNewFolderCancel: () => void
        onAlertClose: () => void
    } = $props()
</script>

{#if showCopyDialog && copyDialogProps}
    <CopyDialog
        sourcePaths={copyDialogProps.sourcePaths}
        destinationPath={copyDialogProps.destinationPath}
        direction={copyDialogProps.direction}
        {volumes}
        currentVolumeId={copyDialogProps.currentVolumeId}
        fileCount={copyDialogProps.fileCount}
        folderCount={copyDialogProps.folderCount}
        sourceFolderPath={copyDialogProps.sourceFolderPath}
        sortColumn={copyDialogProps.sortColumn}
        sortOrder={copyDialogProps.sortOrder}
        sourceVolumeId={copyDialogProps.sourceVolumeId}
        destVolumeId={copyDialogProps.destVolumeId}
        onConfirm={onCopyConfirm}
        onCancel={onCopyCancel}
    />
{/if}

{#if showCopyProgressDialog && copyProgressProps}
    <CopyProgressDialog
        sourcePaths={copyProgressProps.sourcePaths}
        sourceFolderPath={copyProgressProps.sourceFolderPath}
        destinationPath={copyProgressProps.destinationPath}
        direction={copyProgressProps.direction}
        sortColumn={copyProgressProps.sortColumn}
        sortOrder={copyProgressProps.sortOrder}
        previewId={copyProgressProps.previewId}
        sourceVolumeId={copyProgressProps.sourceVolumeId}
        destVolumeId={copyProgressProps.destVolumeId}
        conflictResolution={copyProgressProps.conflictResolution}
        onComplete={onCopyComplete}
        onCancelled={onCopyCancelled}
        onError={onCopyError}
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

{#if showCopyErrorDialog && copyErrorProps}
    <CopyErrorDialog error={copyErrorProps.error} onClose={onCopyErrorClose} />
{/if}
