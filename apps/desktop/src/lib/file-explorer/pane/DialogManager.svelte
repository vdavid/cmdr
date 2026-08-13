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
        AdoptedOperationData,
        TransferProgressPropsData,
        NewFolderDialogPropsData,
        NewFileDialogPropsData,
        AlertDialogPropsData,
        TransferErrorPropsData,
        ArchivePasswordPropsData,
        DeleteDialogPropsData,
    } from './dialog-props'
    import type { ConflictResolution, FriendlyError, TransferOperationType, WriteOperationError } from '../types'

    const {
        onDialogRenderError,
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
        onAdoptedComplete,
        onAdoptedCancelled,
        onAdoptedError,
        onAdoptedQueue,
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
        /** Dismisses every dialog and returns focus to the pane. See `handleRenderError`. */
        onDialogRenderError: (error: unknown) => void
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
            preKnownConflicts: string[],
        ) => void
        onTransferCancel: () => void
        onTransferComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
        onTransferCancelled: (filesProcessed: number) => void
        onTransferError: (error: WriteOperationError, friendly?: FriendlyError) => void
        onTransferQueue: () => void
        /** The four outcomes of a dialog that ADOPTED its operation. Separate
         *  callbacks, not a flag on the started ones: an adopted view has no
         *  birth context, so its tail must not be able to reach the pane work. */
        onAdoptedComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
        onAdoptedCancelled: (filesProcessed: number) => void
        onAdoptedError: (error: WriteOperationError) => void
        onAdoptedQueue: () => void
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

    /**
     * How many consecutive render failures may re-arm the boundary before it
     * stays down. Dismissing every dialog makes all the `{#if}` conditions
     * false, so the retry renders nothing and can't throw again; the cap is
     * there so an unforeseen throw-on-empty can't spin forever.
     */
    const MAX_CONSECUTIVE_RESETS = 3
    /** Failures further apart than this start a fresh count, so one bad moment
     *  doesn't cost the rest of the session its dialogs. A re-render loop lands
     *  well inside it and still trips the cap. */
    const FAILURE_WINDOW_MS = 5000
    let consecutiveFailures = 0
    let lastFailureAt = 0

    /**
     * Recovers from a dialog that threw while rendering.
     *
     * A throw mid-render leaves nothing on screen, but the `show*` flag that
     * opened the dialog is already true, so `isConfirmationDialogOpen()` keeps
     * suppressing the pane's keyboard: the app looks frozen with nothing to
     * escape from. (Lived case: a doubly-mounted NAS put two volumes with one id
     * into the transfer dialog's destination `{#each}`, and Svelte threw
     * `each_key_duplicate` during flush.)
     *
     * So: report it through the app's error path, dismiss every dialog, hand
     * focus back to the pane, then re-arm the boundary. The reset is deferred to
     * the next tick — `setTimeout(0)`, ❌ never `requestAnimationFrame`, which
     * macOS throttles in unfocused windows — so the dismissal has flushed before
     * the contents re-render. Without a reset, the boundary stays failed and NO
     * dialog opens again for the rest of the session.
     */
    function handleRenderError(error: unknown, reset: () => void): void {
        onDialogRenderError(error)
        const now = Date.now()
        consecutiveFailures = now - lastFailureAt > FAILURE_WINDOW_MS ? 1 : consecutiveFailures + 1
        lastFailureAt = now
        if (consecutiveFailures > MAX_CONSECUTIVE_RESETS) return
        setTimeout(() => { reset(); }, 0)
    }
</script>

<!--
    Every dialog renders inside ONE error boundary. A dialog that throws while
    rendering must never leave the app with a suppressed keyboard and nothing on
    screen; `handleRenderError` dismisses them all, gives the pane its focus
    back, and re-arms the boundary. The `failed` snippet renders nothing on
    purpose: by the time it runs there is no dialog left to show.
-->
<svelte:boundary onerror={handleRenderError}>
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
                sortColumn={transferDialogProps.sortColumn}
                sortOrder={transferDialogProps.sortOrder}
                sourceVolumeId={transferDialogProps.sourceVolumeId}
                destVolumeId={transferDialogProps.destVolumeId}
                autoConfirm={transferDialogProps.autoConfirm}
                autoConfirmOnConflict={transferDialogProps.autoConfirmOnConflict}
                mcpRequestId={transferDialogProps.mcpRequestId}
                onConfirm={onTransferConfirm}
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
            direction={transferProgressProps.direction}
            sortColumn={transferProgressProps.sortColumn}
            sortOrder={transferProgressProps.sortOrder}
            previewId={transferProgressProps.previewId}
            sourceVolumeId={transferProgressProps.sourceVolumeId}
            destVolumeId={transferProgressProps.destVolumeId}
            conflictResolution={transferProgressProps.conflictResolution}
            preKnownConflicts={transferProgressProps.preKnownConflicts}
            itemSizes={transferProgressProps.itemSizes}
            mcpRequestId={transferProgressProps.mcpRequestId}
            initiator={transferProgressProps.initiator}
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

    {#snippet failed()}
        <!-- Deliberately empty: the dialogs are dismissed and the pane has focus. -->
    {/snippet}
</svelte:boundary>
