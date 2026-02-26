<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import {
        copyFiles,
        copyBetweenVolumes,
        moveFiles,
        onWriteProgress,
        onWriteComplete,
        onWriteError,
        onWriteCancelled,
        onWriteConflict,
        resolveWriteConflict,
        cancelWriteOperation,
        formatBytes,
        formatDuration,
        DEFAULT_VOLUME_ID,
        type WriteProgressEvent,
        type WriteCompleteEvent,
        type WriteErrorEvent,
        type WriteCancelledEvent,
        type WriteConflictEvent,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type {
        TransferOperationType,
        WriteOperationPhase,
        WriteOperationError,
        SortColumn,
        SortOrder,
        ConflictResolution,
    } from '$lib/file-explorer/types'
    import { formatDate } from '$lib/file-explorer/selection/selection-info-utils'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { getSetting } from '$lib/settings'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'

    /** Returns CSS class for size coloring based on bytes (kb/mb/gb/tb) */
    function getSizeColorClass(bytes: number): string {
        if (bytes < 1024) return 'size-bytes'
        if (bytes < 1024 * 1024) return 'size-kb'
        if (bytes < 1024 * 1024 * 1024) return 'size-mb'
        if (bytes < 1024 * 1024 * 1024 * 1024) return 'size-gb'
        return 'size-tb'
    }

    const log = getAppLogger('transferProgress')

    interface Props {
        operationType: TransferOperationType
        sourcePaths: string[]
        sourceFolderPath: string
        destinationPath: string
        direction: 'left' | 'right'
        /** Current sort column on source pane (files will be processed in this order) */
        sortColumn: SortColumn
        /** Current sort order on source pane */
        sortOrder: SortOrder
        /** Preview scan ID from TransferDialog (for reusing scan results, optional) */
        previewId: string | null
        /** Source volume ID (like "root", "mtp-336592896:65537") */
        sourceVolumeId: string
        /** Destination volume ID */
        destVolumeId: string
        /** Conflict resolution policy from TransferDialog */
        conflictResolution: ConflictResolution
        onComplete: (filesProcessed: number, bytesProcessed: number) => void
        onCancelled: (filesProcessed: number) => void
        onError: (error: WriteOperationError) => void
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
        onComplete,
        onCancelled,
        onError,
    }: Props = $props()

    const operationLabel = $derived(operationType === 'copy' ? 'Copy' : 'Move')
    const operationGerund = $derived(operationType === 'copy' ? 'Copying' : 'Moving')

    // Operation state
    let operationId = $state<string | null>(null)
    let phase = $state<WriteOperationPhase>('scanning')
    let currentFile = $state<string | null>(null)
    let filesDone = $state(0)
    let filesTotal = $state(0)
    let bytesDone = $state(0)
    let bytesTotal = $state(0)
    let startTime = $state(0)
    let isCancelling = $state(false)
    let isRollingBack = $state(false)
    let destroyed = false

    // Events that arrived before we know our operationId (from the command response).
    // Without buffering, a stale event from a previous operation could claim the ID slot first.
    type BufferedEvent =
        | { type: 'progress'; event: WriteProgressEvent }
        | { type: 'complete'; event: WriteCompleteEvent }
        | { type: 'error'; event: WriteErrorEvent }
        | { type: 'cancelled'; event: WriteCancelledEvent }
        | { type: 'conflict'; event: WriteConflictEvent }
    let pendingEvents: BufferedEvent[] = []

    /** Returns true if the event belongs to this operation and should be processed. */
    function filterEvent(entry: BufferedEvent): boolean {
        if (operationId === null) {
            pendingEvents.push(entry)
            return false
        }
        return entry.event.operationId === operationId
    }

    function replayBufferedEvents() {
        const events = pendingEvents
        pendingEvents = []
        for (const entry of events) {
            if (entry.event.operationId !== operationId) continue
            switch (entry.type) {
                case 'progress':
                    handleProgress(entry.event)
                    break
                case 'complete':
                    handleComplete(entry.event)
                    break
                case 'error':
                    handleError(entry.event)
                    break
                case 'cancelled':
                    handleCancelled(entry.event)
                    break
                case 'conflict':
                    handleConflict(entry.event)
                    break
            }
        }
    }

    // Conflict state
    let conflictEvent = $state<WriteConflictEvent | null>(null)
    let isResolvingConflict = $state(false)

    // Calculated stats
    const percentComplete = $derived(bytesTotal > 0 ? (bytesDone / bytesTotal) * 100 : 0)

    // Speed and ETA calculation
    const stats = $derived.by(() => {
        if (startTime === 0 || bytesDone === 0) {
            return { bytesPerSecond: 0, estimatedSecondsRemaining: null }
        }
        const elapsedSeconds = (Date.now() - startTime) / 1000
        const bytesPerSecond = elapsedSeconds > 0 ? bytesDone / elapsedSeconds : 0
        const bytesRemaining = bytesTotal - bytesDone
        const estimatedSecondsRemaining = bytesPerSecond > 0 ? bytesRemaining / bytesPerSecond : null
        return { bytesPerSecond, estimatedSecondsRemaining }
    })

    // Progress stages for visualization — the active phase label adapts to operation type
    const stages = $derived<{ id: WriteOperationPhase; label: string }[]>([
        { id: 'scanning', label: 'Scanning' },
        { id: 'copying', label: operationGerund },
    ])

    function getStageStatus(stageId: WriteOperationPhase): 'done' | 'active' | 'pending' {
        const currentIndex = stages.findIndex((s) => s.id === phase)
        const stageIndex = stages.findIndex((s) => s.id === stageId)

        if (stageIndex < currentIndex) return 'done'
        if (stageIndex === currentIndex) return 'active'
        return 'pending'
    }

    function handleProgress(event: WriteProgressEvent) {
        if (!filterEvent({ type: 'progress', event })) return

        log.debug('Progress event: {phase} {filesDone}/{filesTotal} files, {bytesDone}/{bytesTotal} bytes', {
            phase: event.phase,
            filesDone: event.filesDone,
            filesTotal: event.filesTotal,
            bytesDone: event.bytesDone,
            bytesTotal: event.bytesTotal,
        })

        phase = event.phase
        currentFile = event.currentFile
        filesDone = event.filesDone
        filesTotal = event.filesTotal
        bytesDone = event.bytesDone
        bytesTotal = event.bytesTotal
    }

    function handleComplete(event: WriteCompleteEvent) {
        if (!filterEvent({ type: 'complete', event })) return

        log.info('{op} complete: {filesProcessed} files, {bytesProcessed} bytes', {
            op: operationLabel,
            filesProcessed: event.filesProcessed,
            bytesProcessed: event.bytesProcessed,
        })

        cleanup()
        onComplete(event.filesProcessed, event.bytesProcessed)
    }

    function handleError(event: WriteErrorEvent) {
        if (!filterEvent({ type: 'error', event })) return

        log.error('{op} error: {errorType}', { op: operationLabel, errorType: event.error.type, error: event.error })

        cleanup()
        onError(event.error)
    }

    function handleCancelled(event: WriteCancelledEvent) {
        if (!filterEvent({ type: 'cancelled', event })) return

        log.info('{op} cancelled after {filesProcessed} files, rolledBack={rolledBack}', {
            op: operationLabel,
            filesProcessed: event.filesProcessed,
            rolledBack: event.rolledBack,
        })

        cleanup()
        onCancelled(event.filesProcessed)
    }

    function handleConflict(event: WriteConflictEvent) {
        if (!filterEvent({ type: 'conflict', event })) return

        log.info('Conflict detected: {sourcePath} -> {destinationPath}', {
            sourcePath: event.sourcePath,
            destinationPath: event.destinationPath,
        })

        conflictEvent = event
    }

    async function handleConflictResolution(resolution: ConflictResolution, applyToAll: boolean) {
        if (!operationId || !conflictEvent) return

        log.info('Resolving conflict with {resolution}, applyToAll={applyToAll}', { resolution, applyToAll })

        isResolvingConflict = true
        try {
            await resolveWriteConflict(operationId, resolution, applyToAll)
            conflictEvent = null
        } catch (err) {
            log.error('Failed to resolve conflict: {error}', { error: err })
        } finally {
            isResolvingConflict = false
        }
    }

    // Store multiple unlisteners
    let unlisteners: UnlistenFn[] = []

    function cleanup() {
        log.debug('Cleaning up {count} event listeners', { count: unlisteners.length })
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
    }

    async function startOperation() {
        log.info('Starting {op} operation: {sourceCount} sources to {destination}', {
            op: operationType,
            sourceCount: sourcePaths.length,
            destination: destinationPath,
        })

        startTime = Date.now()

        // CRITICAL: Subscribe to events BEFORE starting the operation to avoid race condition
        // Events are emitted immediately when the operation starts - if we subscribe after,
        // we might miss progress/complete events for fast operations (like single large files)
        log.debug('Subscribing to write events BEFORE starting operation')

        unlisteners.push(await onWriteProgress(handleProgress))
        unlisteners.push(await onWriteComplete(handleComplete))
        unlisteners.push(await onWriteError(handleError))
        unlisteners.push(await onWriteCancelled(handleCancelled))
        unlisteners.push(await onWriteConflict(handleConflict))

        log.debug('Event subscriptions ready, starting {op}', { op: operationType })

        try {
            const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
            const maxConflictsToShow = getSetting('fileOperations.maxConflictsToShow')

            let result

            if (operationType === 'move') {
                // Move always uses moveFiles (backend handles same-fs rename vs cross-fs copy+delete)
                result = await moveFiles(sourcePaths, destinationPath, {
                    conflictResolution,
                    progressIntervalMs,
                    maxConflictsToShow,
                    sortColumn,
                    sortOrder,
                    previewId,
                })
            } else {
                // Copy: use unified copyBetweenVolumes for cross-volume operations (including MTP)
                // Fall back to copyFiles for local-to-local copies when both volumes are "root"
                const isLocalToLocal = sourceVolumeId === DEFAULT_VOLUME_ID && destVolumeId === DEFAULT_VOLUME_ID
                result = isLocalToLocal
                    ? await copyFiles(sourcePaths, destinationPath, {
                          conflictResolution,
                          progressIntervalMs,
                          maxConflictsToShow,
                          sortColumn,
                          sortOrder,
                          previewId,
                      })
                    : await copyBetweenVolumes(sourceVolumeId, sourcePaths, destVolumeId, destinationPath, {
                          conflictResolution,
                          progressIntervalMs,
                          maxConflictsToShow,
                      })
            }

            operationId = result.operationId
            log.info('{op} operation started with operationId: {operationId}', {
                op: operationLabel,
                operationId,
            })

            // If the dialog was destroyed/cancelled while waiting for the IPC response,
            // cancel the operation immediately and bail out
            if (destroyed) {
                log.info('Dialog destroyed before operationId arrived — cancelling op={operationId}', {
                    operationId,
                })
                void cancelWriteOperation(operationId, true)
                cleanup()
                return
            }

            replayBufferedEvents()
        } catch (err: unknown) {
            log.error('Failed to start {op} operation: {error}', { op: operationType, error: err })
            cleanup()
            // Tauri commands return structured WriteOperationError objects on validation failure
            // (e.g. destination_inside_source). Pass them through to preserve the specific error type.
            if (typeof err === 'object' && err !== null && 'type' in err) {
                onError(err as WriteOperationError)
            } else {
                onError({
                    type: 'io_error',
                    path: sourcePaths[0] ?? '',
                    message: `Failed to start ${operationType}: ${String(err)}`,
                })
            }
        }
    }

    async function handleCancel(rollback: boolean) {
        if (!operationId) {
            log.warn('Cancel requested but no operationId yet — will cancel after IPC resolves')
            destroyed = true
            return
        }
        if (isCancelling || isRollingBack) {
            log.debug('Cancel/rollback already in progress')
            return
        }

        if (rollback) {
            // Rollback: keep dialog open, show "Rolling back...", wait for event
            log.info('Rolling back operation: {operationId}', { operationId })
            isRollingBack = true
            isCancelling = true
            try {
                await cancelWriteOperation(operationId, true)
                log.debug('Rollback request sent successfully')
                // Dialog will close when write-cancelled event is received
            } catch (err) {
                log.error('Failed to rollback operation: {error}', { error: err })
                isRollingBack = false
                isCancelling = false
            }
        } else {
            // Cancel: close immediately, keep partial files
            log.info('Cancelling operation (keeping partial files): {operationId}', { operationId })
            isCancelling = true
            try {
                await cancelWriteOperation(operationId, false)
                log.debug('Cancel request sent successfully')
                // Close immediately without waiting for backend confirmation
                cleanup()
                onCancelled(filesDone)
            } catch (err) {
                log.error('Failed to cancel operation: {error}', { error: err })
                isCancelling = false
            }
        }
    }

    function handleKeydown(event: KeyboardEvent) {
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
        void startOperation()
    })

    onDestroy(() => {
        destroyed = true
        if (operationId) {
            // Cancel with rollback on unexpected teardown (hot-reload, navigation, crash).
            // Normal user-initiated cancel/complete already ran cleanup() + callbacks,
            // and the backend ignores cancel on finished operations (idempotent).
            void cancelWriteOperation(operationId, true)
        }
        cleanup()
    })
</script>

<ModalDialog
    titleId="progress-dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-progress"
    onclose={() => void handleCancel(false)}
    containerStyle="min-width: 420px; max-width: 500px"
>
    {#snippet title()}
        {#if isRollingBack}
            Rolling back...
        {:else if conflictEvent}
            File already exists
        {:else}
            {operationGerund}...
        {/if}
    {/snippet}

    {#if conflictEvent}
        <!-- Conflict resolution -->
        {@const fileName = conflictEvent.destinationPath.split('/').pop() ?? ''}
        {@const existingIsNewer = conflictEvent.destinationIsNewer}
        {@const newIsNewer = !existingIsNewer && conflictEvent.sourceModified !== conflictEvent.destinationModified}
        {@const existingIsLarger = conflictEvent.sizeDifference > 0}
        {@const newIsLarger = conflictEvent.sizeDifference < 0}
        <div class="conflict-section">
            <!-- Filename -->
            <p class="conflict-filename" use:tooltip={{ text: conflictEvent.destinationPath, overflowOnly: true }}>
                {fileName}
            </p>

            <!-- File comparison -->
            <div class="conflict-comparison">
                <div class="conflict-file">
                    <span class="conflict-file-label">Existing:</span>
                    <span class="conflict-file-size {getSizeColorClass(conflictEvent.destinationSize)}"
                        >{formatFileSize(conflictEvent.destinationSize)}</span
                    >
                    {#if existingIsLarger}<span class="conflict-annotation larger">(larger)</span>{/if}
                    <span class="conflict-file-date"
                        >{conflictEvent.destinationModified ? formatDate(conflictEvent.destinationModified) : ''}</span
                    >
                    {#if existingIsNewer}<span class="conflict-annotation newer">(newer)</span>{/if}
                </div>
                <div class="conflict-file">
                    <span class="conflict-file-label">New:</span>
                    <span class="conflict-file-size {getSizeColorClass(conflictEvent.sourceSize)}"
                        >{formatFileSize(conflictEvent.sourceSize)}</span
                    >
                    {#if newIsLarger}<span class="conflict-annotation larger">(larger)</span>{/if}
                    <span class="conflict-file-date"
                        >{conflictEvent.sourceModified ? formatDate(conflictEvent.sourceModified) : ''}</span
                    >
                    {#if newIsNewer}<span class="conflict-annotation newer">(newer)</span>{/if}
                </div>
            </div>

            <!-- Question -->
            <p class="conflict-question">Do you want to keep the existing file or overwrite it?</p>

            <!-- Buttons in two rows -->
            <div class="conflict-buttons">
                <div class="conflict-buttons-row">
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('skip', false)}
                        disabled={isResolvingConflict}
                    >
                        Skip
                    </Button>
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('overwrite', false)}
                        disabled={isResolvingConflict}
                    >
                        Overwrite
                    </Button>
                </div>
                <div class="conflict-buttons-row">
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('skip', true)}
                        disabled={isResolvingConflict}
                    >
                        Skip all
                    </Button>
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('overwrite', true)}
                        disabled={isResolvingConflict}
                    >
                        Overwrite all
                    </Button>
                </div>
            </div>

            <!-- Cancel at bottom -->
            <div class="conflict-cancel">
                <button
                    class="danger-text"
                    onclick={() => handleCancel(true)}
                    disabled={isCancelling || isResolvingConflict}
                >
                    Rollback
                </button>
            </div>
        </div>
    {:else if isRollingBack}
        <!-- Rollback in progress -->
        <div class="rollback-section">
            <div class="rollback-indicator">
                <span class="spinner spinner-md rollback-spinner"></span>
            </div>
            <p class="rollback-message">
                Deleting {filesDone}
                {operationType === 'copy' ? 'copied' : 'partially moved'} files...
            </p>
        </div>
    {:else}
        <!-- Direction indicator -->
        <DirectionIndicator sourcePath={sourceFolderPath} {destinationPath} {direction} />

        <!-- Progress stages -->
        <div class="progress-stages">
            {#each stages as stage (stage.id)}
                {@const status = getStageStatus(stage.id)}
                <div class="stage" class:done={status === 'done'} class:active={status === 'active'}>
                    <div class="stage-indicator">
                        {#if status === 'done'}
                            <span class="checkmark">✓</span>
                        {:else if status === 'active'}
                            <span class="spinner spinner-sm stage-spinner"></span>
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

        <!-- Progress bar -->
        <div class="progress-section">
            <div class="progress-bar-container">
                <div class="progress-bar" style="width: {percentComplete}%"></div>
            </div>
            <div class="progress-info">
                <span class="progress-percent">{Math.round(percentComplete)}%</span>
                {#if stats.estimatedSecondsRemaining !== null}
                    <span class="eta">~{formatDuration(stats.estimatedSecondsRemaining)} remaining</span>
                {/if}
            </div>
        </div>

        <!-- Stats -->
        <div class="stats-section">
            <div class="stat-row">
                <span class="stat-label">Files:</span>
                <span class="stat-value">{filesDone} / {filesTotal}</span>
            </div>
            <div class="stat-row">
                <span class="stat-label">Size:</span>
                <span class="stat-value">{formatBytes(bytesDone)} / {formatBytes(bytesTotal)}</span>
            </div>
            {#if stats.bytesPerSecond > 0}
                <div class="stat-row">
                    <span class="stat-label">Speed:</span>
                    <span class="stat-value">{formatBytes(stats.bytesPerSecond)}/s</span>
                </div>
            {/if}
        </div>

        <!-- Current file -->
        {#if currentFile}
            <div class="current-file" use:tooltip={{ text: currentFile, overflowOnly: true }}>
                {currentFile}
            </div>
        {/if}

        <!-- Action buttons -->
        <div class="button-row">
            <span use:tooltip={'Cancel and keep progress'}>
                <Button variant="secondary" onclick={() => handleCancel(false)} disabled={isCancelling}>Cancel</Button>
            </span>
            <span use:tooltip={'Cancel and delete any partial target files created'}>
                <Button variant="danger" onclick={() => handleCancel(true)} disabled={isCancelling}>Rollback</Button>
            </span>
        </div>
    {/if}
</ModalDialog>

<style>
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
        color: var(--color-accent);
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
        font-weight: bold;
    }

    .dot {
        width: 8px;
        height: 8px;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
    }

    .stage-spinner {
        border-color: var(--color-accent);
        border-top-color: transparent;
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

    /* Progress bar */
    .progress-section {
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-md);
    }

    .progress-bar-container {
        width: 100%;
        height: 8px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
        overflow: hidden;
    }

    .progress-bar {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-sm);
        transition: width 0.1s ease-out;
    }

    .progress-info {
        display: flex;
        justify-content: space-between;
        margin-top: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .progress-percent {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .eta {
        color: var(--color-text-tertiary);
    }

    /* Stats */
    .stats-section {
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-md);
    }

    .stat-row {
        display: flex;
        justify-content: space-between;
        font-size: var(--font-size-sm);
        padding: 2px 0;
    }

    .stat-label {
        color: var(--color-text-tertiary);
    }

    .stat-value {
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    /* Current file */
    .current-file {
        padding: var(--spacing-sm) var(--spacing-xl);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        margin: 0 var(--spacing-lg);
        border-radius: var(--radius-sm);
    }

    /* Buttons */
    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: var(--spacing-lg) var(--spacing-xl) 20px;
    }

    /* Rollback section */
    .rollback-section {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-2xl) var(--spacing-xl);
        gap: var(--spacing-lg);
    }

    .rollback-indicator {
        width: 32px;
        height: 32px;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .rollback-spinner {
        border-color: var(--color-error);
        border-top-color: transparent;
    }

    .rollback-message {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        text-align: center;
    }

    /* Conflict section */
    .conflict-section {
        padding: var(--spacing-md) var(--spacing-xl) 20px;
    }

    .conflict-filename {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .conflict-comparison {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-lg);
        font-size: var(--font-size-sm);
    }

    .conflict-file {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        justify-content: center;
        flex-wrap: wrap;
    }

    .conflict-file-label {
        color: var(--color-text-tertiary);
        min-width: 55px;
        text-align: right;
    }

    .conflict-file-size {
        font-weight: 500;
        min-width: 70px;
    }

    .conflict-file-date {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .conflict-annotation {
        font-size: var(--font-size-sm);
        font-weight: 500;
    }

    .conflict-annotation.newer {
        color: var(--color-accent);
    }

    .conflict-annotation.larger {
        color: var(--color-size-mb);
    }

    .conflict-question {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .conflict-buttons {
        display: flex;
        flex-direction: column;
        gap: 8px;
        margin-bottom: 16px;
    }

    .conflict-buttons-row {
        display: flex;
        gap: 8px;
        justify-content: center;
    }

    .conflict-buttons :global(button) {
        flex: 1;
        max-width: 120px;
    }

    .conflict-cancel {
        display: flex;
        justify-content: center;
        padding-top: 12px;
        border-top: 1px solid var(--color-border-strong);
    }

    /* Text-only danger button (for less prominent cancel) */
    .danger-text {
        background: transparent;
        color: var(--color-error);
        border: none;
        font-size: var(--font-size-sm);
        font-weight: 500;
        padding: var(--spacing-sm) var(--spacing-lg);
        cursor: pointer;
        transition: all var(--transition-base);
    }

    .danger-text:disabled {
        opacity: 0.4;
        cursor: not-allowed;
    }

    .danger-text:hover:not(:disabled) {
        text-decoration: underline;
    }
</style>
