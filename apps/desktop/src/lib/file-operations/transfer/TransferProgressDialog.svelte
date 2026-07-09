<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import {
        formatDuration,
        formatFilesPerSecond,
        DEFAULT_VOLUME_ID,
    } from '$lib/tauri-commands'
    import type {
        TransferOperationType,
        WriteOperationPhase,
        WriteOperationError,
        SortColumn,
        SortOrder,
        ConflictResolution,
    } from '$lib/file-explorer/types'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import Size from '$lib/ui/Size.svelte'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import { deriveTransferLabel } from './transfer-dialog-utils'
    import ScanPhaseBody from './ScanPhaseBody.svelte'
    import TransferConflictDialog from './TransferConflictDialog.svelte'
    import { createTransferProgressState } from './transfer-progress-state.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'

    interface Props {
        operationType: TransferOperationType
        sourcePaths: string[]
        sourceFolderPath: string
        /** Destination path (not applicable for delete/trash) */
        destinationPath?: string
        /** Transfer direction (not applicable for delete/trash) */
        direction?: 'left' | 'right'
        /** Current sort column on source pane (files will be processed in this order) */
        sortColumn: SortColumn
        /** Current sort order on source pane */
        sortOrder: SortOrder
        /** Preview scan ID from TransferDialog (for reusing scan results, optional) */
        previewId: string | null
        /** Source volume ID (like "root", "mtp-336592896:65537") */
        sourceVolumeId: string
        /** Destination volume ID (not applicable for delete/trash) */
        destVolumeId?: string
        /** Conflict resolution policy from TransferDialog (not applicable for delete/trash) */
        conflictResolution?: ConflictResolution
        /** Source filenames known to conflict at dest (from TransferDialog's pre-flight scan).
         *  Forwarded to the BE so it can bulk-skip them upfront under `Skip all`. */
        preKnownConflicts?: string[]
        /** Per-item sizes for trash progress (from scan or drive index, optional) */
        itemSizes?: number[]
        /** Whether the scan preview is still running (this dialog should subscribe to scan events) */
        scanInProgress?: boolean
        onComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
        onCancelled: (filesProcessed: number) => void
        onError: (error: WriteOperationError) => void
        /** Send this operation to the background: unmount the modal but keep the
         *  op running, managed in the queue window. Fired by the Queue button, the
         *  dialog-scoped F2, and the auto-queue path (an op admitted as Queued).
         *  Optional so existing callers/tests that don't background stay valid. */
        onQueue?: () => void
        /** MCP round-trip id, present only for an auto-confirmed MCP op. Passed to
         *  the state machine so it replies with the spawned operationId. */
        mcpRequestId?: string
    }

    const {
        operationType,
        sourcePaths,
        sourceFolderPath,
        destinationPath,
        direction,
        sortColumn,
        sortOrder,
        previewId,
        sourceVolumeId,
        destVolumeId,
        conflictResolution,
        preKnownConflicts,
        itemSizes,
        scanInProgress = false,
        onComplete,
        onCancelled,
        onError,
        onQueue,
        mcpRequestId,
    }: Props = $props()

    /** The select discriminator the catalog's gerund/verb messages key on. */
    const gerundKind = $derived(operationType)
    const isDeleteOrTrash = $derived(operationType === 'delete' || operationType === 'trash')
    const isCopy = $derived(operationType === 'copy')
    const isMove = $derived(operationType === 'move')

    /** Title for the scanning phase: names the upcoming action so the user
     *  knows why we're walking the tree, not just "scanning for fun". */
    const scanTitleMap: Record<Exclude<TransferOperationType, 'archive_edit'>, MessageKey> = {
        copy: 'fileOperations.transferProgress.scanTitleCopy',
        move: 'fileOperations.transferProgress.scanTitleMove',
        delete: 'fileOperations.transferProgress.scanTitleDelete',
        trash: 'fileOperations.transferProgress.scanTitleTrash',
        compress: 'fileOperations.transferProgress.scanTitleCompress',
    }
    // Archive edits have no scan phase, so no scan title ever renders for them.
    const scanTitle = $derived(operationType === 'archive_edit' ? '' : tString(scanTitleMap[operationType]))
    const volumes = $derived(getVolumes())
    const destUsesNativeSmb = $derived(
        volumes.find((v) => v.id === destVolumeId)?.smbConnectionState === 'os_mount',
    )

    // Source/destination labels for the direction header. At a volume root the
    // path basename isn't a user-meaningful name — for an MTP storage root it's
    // the raw storage id (like "65538"). `deriveTransferLabel` falls back to the
    // volume's display name in that case (like "Virtual Pixel 9 - SD Card").
    const sourceVolume = $derived(volumes.find((v) => v.id === sourceVolumeId))
    const destVolume = $derived(volumes.find((v) => v.id === destVolumeId))
    const sourceLabel = $derived(
        deriveTransferLabel(sourceFolderPath, sourceVolume?.path ?? '/', sourceVolume?.name ?? ''),
    )
    const destinationLabel = $derived(
        deriveTransferLabel(destinationPath ?? '/', destVolume?.path ?? '/', destVolume?.name ?? ''),
    )

    /** A move where source and destination are the SAME non-default volume (one
     *  smb2 share / one MTP device). The backend handles these as a server-side
     *  rename-merge with NO rollback support — it stops without reversing and
     *  reports `rolled_back: false`. Local→local same-FS moves DO have real
     *  rollback (via `MoveTransaction`), so the default local volume is excluded.
     *  Drives the disabled Rollback affordance + tooltip. */
    const isSameVolumeMove = $derived(
        operationType === 'move' &&
            sourceVolumeId !== DEFAULT_VOLUME_ID &&
            sourceVolumeId === (destVolumeId ?? sourceVolumeId),
    )

    const ROLLBACK_UNAVAILABLE_TOOLTIP = $derived(
        tString('fileOperations.transferProgress.rollbackUnavailableTooltip'),
    )

    // Execution state machine (event coordination, phases, cancel/settle,
    // pause/queue, conflict prompt, scan-wait). Lives in a factory so it's
    // testable without rendering; the markup reads its `$state` through the
    // aliases below, exactly as it did when the state was inline.
    const progress = createTransferProgressState({
        operationType,
        sourcePaths,
        destinationPath,
        sortColumn,
        sortOrder,
        previewId,
        sourceVolumeId,
        destVolumeId,
        conflictResolution,
        preKnownConflicts,
        itemSizes,
        scanInProgress,
        onComplete,
        onCancelled,
        onError,
        onQueue,
        mcpRequestId,
    })

    // Local aliases over the factory getters so the markup reads the same names
    // it always has. Each tracks the factory's reactive `$state`, so the template
    // updates exactly as before.
    const waitingForScan = $derived(progress.waitingForScan)
    const phase = $derived(progress.phase)
    const isRollingBack = $derived(progress.isRollingBack)
    const isCancelling = $derived(progress.isCancelling)
    const cancelEventReceived = $derived(progress.cancelEventReceived)
    const settleSlow = $derived(progress.settleSlow)
    const conflictEvent = $derived(progress.conflictEvent)
    const isPaused = $derived(progress.isPaused)
    const pauseInFlight = $derived(progress.pauseInFlight)
    const canPauseOrQueue = $derived(progress.canPauseOrQueue)
    const operationSettled = $derived(progress.operationSettled)
    const isResolvingConflict = $derived(progress.isResolvingConflict)
    const scanFilesFound = $derived(progress.scanFilesFound)
    const scanDirsFound = $derived(progress.scanDirsFound)
    const scanBytesFound = $derived(progress.scanBytesFound)
    const scanCurrentDir = $derived(progress.scanCurrentDir)
    const scanFilesPerSec = $derived(progress.scanFilesPerSec)
    const scanBytesPerSec = $derived(progress.scanBytesPerSec)
    const currentFile = $derived(progress.currentFile)
    const filesDone = $derived(progress.filesDone)
    const filesTotal = $derived(progress.filesTotal)
    const bytesDone = $derived(progress.bytesDone)
    const bytesTotal = $derived(progress.bytesTotal)
    const bytesPerSecond = $derived(progress.bytesPerSecond)
    const filesPerSecond = $derived(progress.filesPerSecond)
    const etaSecondsDisplay = $derived(progress.etaSecondsDisplay)

    // Progress stages for visualization; the active phase label adapts to operation type.
    const activePhaseId = $derived<WriteOperationPhase>(
        operationType === 'delete' ? 'deleting' : operationType === 'trash' ? 'trashing' : 'copying',
    )
    const stages = $derived<{ id: WriteOperationPhase; label: string }[]>([
        { id: 'scanning', label: tString('fileOperations.transferProgress.stageScanning') },
        { id: activePhaseId, label: tString('fileOperations.transferProgress.stageActive', { gerund: gerundKind }) },
    ])

    function getStageStatus(stageId: WriteOperationPhase): 'done' | 'active' | 'pending' {
        // During rollback OR the closing flush, keep the active phase
        // (copying/moving) marked as still active — flushing is the tail of the
        // copy, not a separate stage chip, so both map back to `activePhaseId`.
        const effectivePhase = phase === 'rolling_back' || phase === 'flushing' ? activePhaseId : phase
        const currentIndex = stages.findIndex((s) => s.id === effectivePhase)
        const stageIndex = stages.findIndex((s) => s.id === stageId)

        if (stageIndex < currentIndex) return 'done'
        if (stageIndex === currentIndex) return 'active'
        return 'pending'
    }

    function handleKeydown(event: KeyboardEvent) {
        // Dialog-scoped F2 → "Queue" (send to background). This is Total
        // Commander's copy-dialog-local F2, NOT the global `file.rename` binding:
        // it works ONLY while this dialog is open and intercepts here. The
        // `ModalDialog` overlay `stopPropagation`s every keydown before it can
        // reach the global key handler, so closing the dialog unmounts this
        // handler and F2 falls through to `file.rename` again (no leak). We still
        // `preventDefault` so the key never triggers a default browser action.
        if (event.key === 'F2' && canPauseOrQueue) {
            event.preventDefault()
            progress.handleQueue()
            return
        }

        if (event.key === 'Tab') {
            // Trap focus within the dialog
            const overlay = event.currentTarget as HTMLElement
            const focusableElements = overlay.querySelectorAll<HTMLElement>(
                'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
            )
            if (focusableElements.length === 0) return

            const firstElement = focusableElements[0]
            const lastElement = focusableElements[focusableElements.length - 1]

            if (event.shiftKey) {
                if (document.activeElement === firstElement) {
                    event.preventDefault()
                    lastElement.focus()
                }
            } else {
                if (document.activeElement === lastElement) {
                    event.preventDefault()
                    firstElement.focus()
                }
            }
        }
    }

    onMount(() => {
        progress.start()
    })

    onDestroy(() => {
        progress.destroy()
    })
</script>

<ModalDialog
    titleId="progress-dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-progress"
    onclose={() => {
        void progress.handleCancel(false)
    }}
    containerStyle="width: 500px"
>
    {#snippet title()}
        {#if waitingForScan}
            {scanTitle}
        {:else if isRollingBack}
            {tString('fileOperations.transferProgress.titleRollingBack')}
        {:else if isCancelling || cancelEventReceived}
            {#if settleSlow}
                {tString('fileOperations.transferProgress.titleCancellingSlow')}
            {:else}
                {tString('fileOperations.transferProgress.titleCancelling')}
            {/if}
        {:else if conflictEvent}
            {tString('fileOperations.transferProgress.titleConflict')}
        {:else if isPaused}
            {tString('fileOperations.transferProgress.titlePaused')}
        {:else if phase === 'flushing'}
            {tString('fileOperations.transferProgress.titleFlushing')}
        {:else}
            {tString('fileOperations.transferProgress.titleActive', { gerund: gerundKind })}
        {/if}
    {/snippet}

    {#if waitingForScan}
        <!-- Scan preview in progress (picked up from TransferDialog) -->
        {#if !isDeleteOrTrash && destinationPath && direction}
            <DirectionIndicator
                sourcePath={sourceFolderPath}
                {destinationPath}
                {direction}
                {sourceLabel}
                {destinationLabel}
            />
        {/if}

        <div class="scan-wait-section">
            <ScanPhaseBody
                {sourceFolderPath}
                {scanFilesFound}
                {scanDirsFound}
                {scanBytesFound}
                {scanFilesPerSec}
                {scanBytesPerSec}
                {scanCurrentDir}
                {currentFile}
            />
        </div>

        <div class="button-row">
            <Button
                variant="secondary"
                onclick={() => {
                    void progress.handleCancel(false)
                }}>{tString('fileOperations.button.cancel')}</Button
            >
        </div>
    {:else if !isDeleteOrTrash && conflictEvent}
        <TransferConflictDialog
            {conflictEvent}
            {isCopy}
            {isMove}
            {isSameVolumeMove}
            {isCancelling}
            {isResolvingConflict}
            onResolve={(resolution: ConflictResolution, applyToAll: boolean) => {
                void progress.handleConflictResolution(resolution, applyToAll)
            }}
            onCancel={(rollback: boolean) => {
                void progress.handleCancel(rollback)
            }}
        />
    {:else}
        <!-- Direction indicator (copy/move only) -->
        {#if !isDeleteOrTrash && destinationPath && direction}
            <DirectionIndicator
                sourcePath={sourceFolderPath}
                {destinationPath}
                {direction}
                {sourceLabel}
                {destinationLabel}
            />
        {/if}

        <!-- Progress stages -->
        <div class="progress-stages">
            {#each stages as stage (stage.id)}
                {@const status = getStageStatus(stage.id)}
                <div class="stage" class:done={status === 'done'} class:active={status === 'active'}>
                    <div class="stage-indicator">
                        {#if status === 'done'}
                            <span class="checkmark">&#10003;</span>
                        {:else if status === 'active'}
                            <Spinner size="sm" />
                        {:else}
                            <span class="dot"></span>
                        {/if}
                    </div>
                    <span>{stage.label}</span>
                </div>
                {#if stage.id !== stages[stages.length - 1].id}
                    <div class="stage-connector" class:done={status === 'done'}></div>
                {/if}
            {/each}
        </div>

        {#if phase === 'scanning'}
            <!-- Scanning phase: tallies, throughput, current dir/file. -->
            <div class="scan-wait-section">
                <ScanPhaseBody
                    {sourceFolderPath}
                    {scanFilesFound}
                    {scanDirsFound}
                    {scanBytesFound}
                    {scanFilesPerSec}
                    {scanBytesPerSec}
                    {scanCurrentDir}
                    {currentFile}
                />
            </div>
        {:else}
            <!-- Dual progress bars (size + count) for the active phase. -->
            <div class="progress-grid">
                {#if bytesTotal > 0}
                    <span class="progress-label">{tString('fileOperations.transferProgress.progressSize')}</span>
                    <ProgressBar
                        value={bytesDone / bytesTotal}
                        ariaLabel={tString('fileOperations.transferProgress.sizeProgressAria')}
                    />
                    <span class="progress-detail">
                        <Size bytes={bytesDone} /> / <Size bytes={bytesTotal} />
                        ({Math.round((bytesDone / bytesTotal) * 100)}%)
                    </span>
                {/if}

                <span class="progress-label"
                    >{operationType === 'trash'
                        ? tString('fileOperations.transferProgress.progressItems')
                        : tString('fileOperations.transferProgress.progressFiles')}</span
                >
                <ProgressBar
                    value={filesTotal > 0 ? filesDone / filesTotal : 0}
                    ariaLabel={tString('fileOperations.transferProgress.fileProgressAria')}
                />
                <span class="progress-detail">{formatNumber(filesDone)} / {formatNumber(filesTotal)}</span>
                <div class="progress-meta">
                    <span class="progress-speeds">
                        {#if bytesPerSecond !== null && bytesPerSecond > 0}
                            <span class="progress-speed"
                                ><Trans key="fileOperations.shared.byteRate" snippets={{ size: byteRateSize }} /></span
                            >
                        {/if}
                        {#if filesPerSecond !== null}
                            {@const filesPerSecLabel = formatFilesPerSecond(filesPerSecond)}
                            {#if filesPerSecLabel !== null}
                                <span class="progress-speed">{filesPerSecLabel}</span>
                            {/if}
                        {/if}
                    </span>
                    {#if etaSecondsDisplay !== null}
                        <span class="progress-eta"
                            >{tString('fileOperations.transferProgress.etaRemaining', {
                                duration: formatDuration(etaSecondsDisplay),
                            })}</span
                        >
                    {/if}
                </div>
            </div>

            <!-- Current file (active phase only; scanning shows it inside scanPhaseBody) -->
            {#if currentFile}
                <div class="current-file" use:useShortenMiddle={{ text: currentFile, preferBreakAt: '/' }}>
                </div>
            {/if}
        {/if}

        {#if destUsesNativeSmb}
            <p class="smb-native-note">
                {tString('fileOperations.transferProgress.smbNativeNote')}
            </p>
        {/if}

        <!-- Action buttons -->
        <!-- Once `operationSettled` is true (write-complete / write-cancelled / write-error
             arrived) the backend state is gone, so a Rollback click can't be honored; disable
             both buttons during the MIN_DISPLAY_MS hold-open window. Without this, the user can
             click Rollback after the copy completed and silently get nothing. -->
        <div class="button-row">
            <!-- Manage controls: Pause/Resume keeps the op alive but parked;
                 Queue sends it to the background and opens the queue window (also
                 F2 while this dialog is focused). Both show only during the active
                 copy/move/delete phases (`canPauseOrQueue`). -->
            {#if canPauseOrQueue}
                <Button
                    variant="secondary"
                    onclick={progress.handlePauseResume}
                    disabled={pauseInFlight}
                    aria-label={isPaused
                        ? tString('fileOperations.transferProgress.resumeAria')
                        : tString('fileOperations.transferProgress.pauseAria')}
                >
                    <span class="btn-inner">
                        <Icon name={isPaused ? 'play' : 'pause'} size={14} />
                        {isPaused
                            ? tString('fileOperations.transferProgress.resume')
                            : tString('fileOperations.transferProgress.pause')}
                    </span>
                </Button>
                <span use:tooltip={tString('fileOperations.transferProgress.queueTooltip')}>
                    <Button
                        variant="secondary"
                        onclick={progress.handleQueue}
                        aria-label={tString('fileOperations.transferProgress.queueAria')}
                    >
                        <span class="btn-inner">
                            <Icon name="list" size={14} />
                            {tString('fileOperations.transferProgress.queue')}
                        </span>
                    </Button>
                </span>
            {/if}
            <Button
                variant="secondary"
                onclick={() => progress.handleCancel(false)}
                disabled={isCancelling || operationSettled}>{tString('fileOperations.button.cancel')}</Button
            >
            {#if isCopy || isMove}
                {#if isRollingBack}
                    <Button variant="danger" disabled>{tString('fileOperations.transferProgress.titleRollingBack')}</Button
                    >
                {:else if isSameVolumeMove}
                    <!-- Same-volume volume moves have no backend rollback; the
                         button is disabled with an explanatory tooltip. Plain
                         Cancel above stays reachable. -->
                    <span use:tooltip={ROLLBACK_UNAVAILABLE_TOOLTIP}>
                        <Button variant="danger" disabled
                            >{tString('fileOperations.transferProgress.conflictRollback')}</Button
                        >
                    </span>
                {:else}
                    <span use:tooltip={tString('fileOperations.transferProgress.rollbackTooltip')}>
                        <Button
                            variant="danger"
                            onclick={() => progress.handleCancel(true)}
                            disabled={isCancelling || operationSettled}
                            >{tString('fileOperations.transferProgress.conflictRollback')}</Button
                        >
                    </span>
                {/if}
            {/if}
        </div>
    {/if}
</ModalDialog>

{#snippet byteRateSize(children: import('svelte').Snippet)}<Size bytes={bytesPerSecond ?? 0} />{@render children()}{/snippet}

<style>
    /* Scan wait section (wraps the ScanPhaseBody child during the scan phases) */
    .scan-wait-section {
        padding: var(--spacing-md) var(--spacing-xl) var(--spacing-lg);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    /* Progress stages */
    .progress-stages {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-md) var(--spacing-xl);
        gap: var(--spacing-sm);
    }

    .stage {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        transition: color var(--transition-slow);
    }

    .stage.active {
        color: var(--color-accent-text);
    }

    .stage.done {
        color: var(--color-allow);
    }

    .stage-indicator {
        width: 18px;
        height: 18px;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .checkmark {
        font-size: var(--font-size-md);
        font-weight: 600;
    }

    .dot {
        width: 8px;
        height: 8px;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
    }

    .stage-connector {
        width: 24px;
        height: 2px;
        background: var(--color-border-strong);
        transition: background var(--transition-slow);
    }

    .stage-connector.done {
        background: var(--color-allow);
    }

    /* Dual progress bars */
    .progress-grid {
        display: grid;
        grid-template-columns: auto 1fr auto;
        gap: var(--spacing-xs) var(--spacing-sm);
        align-items: center;
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-md);
    }

    .progress-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .progress-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
        text-align: right;
    }

    .progress-meta {
        grid-column: 1 / -1;
        display: flex;
        justify-content: space-between;
        font-size: var(--font-size-sm);
    }

    .progress-speeds {
        display: inline-flex;
        gap: var(--spacing-sm);
    }

    .progress-speed {
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    .progress-eta {
        color: var(--color-text-tertiary);
    }

    /* Current file */
    .current-file {
        padding: var(--spacing-sm) var(--spacing-xl);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        margin: 0 var(--spacing-lg);
        border-radius: var(--radius-sm);
    }

    /* Buttons */
    .smb-native-note {
        margin: 0 var(--spacing-xl);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
        border-radius: var(--radius-sm);
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
        flex-wrap: wrap;
        padding: var(--spacing-lg) var(--spacing-xl) var(--spacing-xl);
    }

    /* Icon + label inside the Pause/Resume and Queue buttons. */
    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
