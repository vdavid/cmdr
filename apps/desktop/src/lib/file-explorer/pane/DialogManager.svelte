<script lang="ts">
    import TransferDialog from '../../file-operations/transfer/TransferDialog.svelte'
    import TransferProgressDialog from '../../file-operations/transfer/TransferProgressDialog.svelte'
    import TransferErrorDialog from '../../file-operations/transfer/TransferErrorDialog.svelte'
    import UnlockDialog from '../../file-operations/transfer/UnlockDialog.svelte'
    import DeleteDialog from '#lib/file-operations/delete/DeleteDialog.svelte'
    import NewFolderDialog from '#lib/file-operations/mkdir/NewFolderDialog.svelte'
    import NewFileDialog from '#lib/file-operations/mkfile/NewFileDialog.svelte'
    import AlertDialog from '#lib/ui/AlertDialog.svelte'
    import DialogBoundary from '#lib/ui/boundary/DialogBoundary.svelte'
    import type { TransferDialogPropsData } from './transfer-operations'
    import type {
        AdoptedOperationData,
        TransferProgressPropsData,
        NewFolderDialogPropsData,
        NewFileDialogPropsData,
        AlertDialogPropsData,
        TransferErrorPropsData,
        UnlockPromptPropsData,
        DeleteDialogPropsData,
        TransferConfirmPayload,
        TransferConfirmer,
        DeleteConfirmer,
        ConfirmOrigin,
        TransferCompletePayload,
    } from './dialog-props'
    import type { FriendlyError, WriteOperationError } from '../types'
    import type { ProgressAtStop } from '#lib/tauri-commands/index.js'

    const {
        onDismissAll,
        showTransferDialog,
        transferDialogProps,
        showTransferProgressDialog,
        transferProgressProps,
        adoptedProgressProps,
        showNewFolderDialog,
        newFolderDialogProps,
        showNewFileDialog,
        newFileDialogProps,
        showAlertDialog,
        alertDialogProps,
        showTransferErrorDialog,
        transferErrorProps,
        showUnlockDialog,
        unlockProps,
        showDeleteDialog,
        deleteDialogProps,
        onTransferConfirm,
        registerTransferConfirmer,
        registerDeleteConfirmer,
        onTransferCancel,
        onTransferComplete,
        onTransferCancelled,
        onTransferError,
        onTransferQueue,
        onAdoptedComplete,
        onAdoptedCancelled,
        onAdoptedError,
        onAdoptedQueue,
        onTransferErrorClose,
        onTransferErrorRetry,
        onTransferErrorCopyAnyway,
        onUnlockSubmit,
        onUnlockCancel,
        onNewFolderCreated,
        onNewFolderCancel,
        onNewFileCreated,
        onNewFileCancel,
        onAlertClose,
        onDeleteConfirm,
        onTrashInBackground,
        onDeleteCancel,
    }: {
        /** Dismisses every dialog and returns focus to the pane, after one of them threw. */
        onDismissAll: () => void
        showTransferDialog: boolean
        transferDialogProps: TransferDialogPropsData | null
        showTransferProgressDialog: boolean
        transferProgressProps: TransferProgressPropsData | null
        adoptedProgressProps: AdoptedOperationData | null
        showNewFolderDialog: boolean
        newFolderDialogProps: NewFolderDialogPropsData | null
        showNewFileDialog: boolean
        newFileDialogProps: NewFileDialogPropsData | null
        showAlertDialog: boolean
        alertDialogProps: AlertDialogPropsData | null
        showTransferErrorDialog: boolean
        transferErrorProps: TransferErrorPropsData | null
        showUnlockDialog: boolean
        unlockProps: UnlockPromptPropsData | null
        showDeleteDialog: boolean
        deleteDialogProps: DeleteDialogPropsData | null
        onTransferConfirm: (payload: TransferConfirmPayload) => void
        /** Hands the mounted transfer dialog's own confirm to the dialog state, for the MCP `dialog confirm`. */
        registerTransferConfirmer: (confirm: TransferConfirmer) => () => void
        /** The delete dialog's twin of `registerTransferConfirmer`. */
        registerDeleteConfirmer: (confirm: DeleteConfirmer) => () => void
        onTransferCancel: () => void
        onTransferComplete: (payload: TransferCompletePayload) => void
        onTransferCancelled: (filesProcessed: number) => void
        onTransferError: (
            error: WriteOperationError,
            progressAtStop: ProgressAtStop | null,
            friendly?: FriendlyError,
        ) => void
        onTransferQueue: (operationId: string) => void
        /** The four outcomes of a dialog that ADOPTED its operation. Separate
         *  callbacks, not a flag on the started ones: an adopted view has no
         *  birth context, so its tail must not be able to reach the pane work. */
        onAdoptedComplete: (payload: TransferCompletePayload) => void
        onAdoptedCancelled: (filesProcessed: number) => void
        onAdoptedError: (error: WriteOperationError, progressAtStop: ProgressAtStop | null) => void
        onAdoptedQueue: () => void
        onTransferErrorClose: () => void
        /** The error dialog's Retry. Offered only when `transferErrorProps.retry` holds something to start. */
        onTransferErrorRetry?: () => void
        /** The error dialog's "Copy anyway" after a space shortfall. Offered only when `transferErrorProps.retry` holds the copy to start. */
        onTransferErrorCopyAnyway?: () => void
        onUnlockSubmit: (answer: { password: string; remember: boolean }) => void
        onUnlockCancel: () => void
        onNewFolderCreated: (folderName: string) => void
        onNewFolderCancel: () => void
        onNewFileCreated: (fileName: string) => void
        onNewFileCancel: () => void
        onAlertClose: () => void
        onDeleteConfirm: (previewId: string | null, isPermanent: boolean, origin?: ConfirmOrigin) => void
        /** The delete dialog's F2 / Background button: always a trash, never a delete. */
        onTrashInBackground: (previewId: string | null, origin?: ConfirmOrigin) => void
        onDeleteCancel: () => void
    } = $props()
</script>

<!--
    Every dialog renders inside ONE `DialogBoundary`. A dialog that throws while
    rendering or tearing down must never leave the app with a suppressed keyboard
    and nothing on screen: the `show*` flag that opened it is already true, so
    `isConfirmationDialogOpen()` keeps eating the pane's keys. (Lived case: a
    doubly-mounted NAS put two volumes with one id into the transfer dialog's
    destination `{#each}`, and Svelte threw `each_key_duplicate` during flush.)
    So the boundary's close dismisses them ALL, which also hands the pane its
    focus back; the boundary reports, toasts, and re-arms.
-->
<DialogBoundary name="paneDialogs" onClose={onDismissAll}>
    {#if showTransferDialog && transferDialogProps}
        {#key transferDialogProps}
            <TransferDialog
                operationType={transferDialogProps.operationType}
                sourcePaths={transferDialogProps.sourcePaths}
                destinationPath={transferDialogProps.destinationPath}
                currentVolumeId={transferDialogProps.currentVolumeId}
                fileCount={transferDialogProps.fileCount}
                folderCount={transferDialogProps.folderCount}
                sourceFolderPath={transferDialogProps.sourceFolderPath}
                sourceVolumeId={transferDialogProps.sourceVolumeId}
                destVolumeId={transferDialogProps.destVolumeId}
                autoConfirm={transferDialogProps.autoConfirm}
                autoConfirmOnConflict={transferDialogProps.autoConfirmOnConflict}
                mcpRequestId={transferDialogProps.mcpRequestId}
                newName={transferDialogProps.newName}
                onConfirm={onTransferConfirm}
                registerConfirmer={registerTransferConfirmer}
                onCancel={onTransferCancel}
            />
        {/key}
    {/if}

    <!--
        Two arms, one dialog, because the two are genuinely different things: one
        STARTS an operation from birth context and may act on the panes
        afterwards, the other only WATCHES one that started elsewhere and must
        not. The callbacks differ by design — an adopted view's outcomes touch no
        pane. See `dialog-state.svelte.ts` § "Birth context".

        ❌ Keep them ONE chain rather than two sibling `{#if}`s. `foregroundOperation`
        refuses an occupied slot, so only one set of props is ever filled; the
        `{:else if}` is what makes stacking two progress dialogs over a user's
        transfer unreachable instead of a convention held in another file.
    -->
    {#if showTransferProgressDialog && adoptedProgressProps}
        <TransferProgressDialog
            adoptOperationId={adoptedProgressProps.operationId}
            operationType={adoptedProgressProps.operationType}
            reverses={adoptedProgressProps.reverses}
            sourceFolderPath={adoptedProgressProps.sourcePath ?? ''}
            destinationPath={adoptedProgressProps.destinationPath ?? undefined}
            onComplete={onAdoptedComplete}
            onCancelled={onAdoptedCancelled}
            onError={onAdoptedError}
            onQueue={onAdoptedQueue}
        />
    {:else if showTransferProgressDialog && transferProgressProps}
        <TransferProgressDialog
            operationType={transferProgressProps.operationType}
            sourcePaths={transferProgressProps.sourcePaths}
            sourceFolderPath={transferProgressProps.sourceFolderPath}
            destinationPath={transferProgressProps.destinationPath}
            destinationName={transferProgressProps.destinationName}
            direction={transferProgressProps.direction}
            previewId={transferProgressProps.previewId}
            sourceVolumeId={transferProgressProps.sourceVolumeId}
            destVolumeId={transferProgressProps.destVolumeId}
            conflictResolution={transferProgressProps.conflictResolution}
            preKnownConflicts={transferProgressProps.preKnownConflicts}
            spaceShortfall={transferProgressProps.spaceShortfall}
            itemSizes={transferProgressProps.itemSizes}
            fileCount={transferProgressProps.fileCount}
            folderCount={transferProgressProps.folderCount}
            mcpRequestId={transferProgressProps.mcpRequestId}
            initiator={transferProgressProps.initiator}
            newName={transferProgressProps.newName}
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
                cloudOnlineOnly={deleteDialogProps.cloudOnlineOnly}
                cloudFolderMayHoldOnlineOnly={deleteDialogProps.cloudFolderMayHoldOnlineOnly}
                isFromCursor={deleteDialogProps.isFromCursor}
                sourceVolumeId={deleteDialogProps.sourceVolumeId}
                autoConfirm={deleteDialogProps.autoConfirm}
                mcpRequestId={deleteDialogProps.mcpRequestId}
                onConfirm={onDeleteConfirm}
                onConfirmInBackground={onTrashInBackground}
                registerConfirmer={registerDeleteConfirmer}
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
            initiator={newFolderDialogProps.initiator}
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
            initiator={newFileDialogProps.initiator}
            onCreated={onNewFileCreated}
            onCancel={onNewFileCancel}
        />
    {/if}

    {#if showAlertDialog && alertDialogProps}
        <AlertDialog
            title={alertDialogProps.title}
            message={alertDialogProps.message}
            path={alertDialogProps.path}
            onClose={onAlertClose}
        />
    {/if}

    {#if showTransferErrorDialog && transferErrorProps}
        <TransferErrorDialog
            operationType={transferErrorProps.operationType}
            error={transferErrorProps.error}
            progressAtStop={transferErrorProps.progressAtStop}
            onClose={onTransferErrorClose}
            onRetry={transferErrorProps.retry ? onTransferErrorRetry : undefined}
            onCopyAnyway={transferErrorProps.retry ? onTransferErrorCopyAnyway : undefined}
        />
    {/if}

    {#if showUnlockDialog && unlockProps}
        <UnlockDialog
            container={unlockProps.container}
            name={unlockProps.name}
            wrongAttempt={unlockProps.wrongAttempt}
            checking={unlockProps.checking}
            unreachable={unlockProps.unreachable}
            rejections={unlockProps.rejections}
            onSubmit={onUnlockSubmit}
            onCancel={onUnlockCancel}
        />
    {/if}
</DialogBoundary>
