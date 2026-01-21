<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        copyFiles,
        onWriteProgress,
        onWriteComplete,
        onWriteError,
        onWriteCancelled,
        cancelWriteOperation,
        formatBytes,
        formatDuration,
        type WriteProgressEvent,
        type WriteCompleteEvent,
        type WriteErrorEvent,
        type WriteCancelledEvent,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { WriteOperationPhase, WriteOperationError, SortColumn, SortOrder } from '$lib/file-explorer/types'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('copyProgress')

    interface Props {
        sourcePaths: string[]
        sourceFolderPath: string
        destinationPath: string
        direction: 'left' | 'right'
        /** Current sort column on source pane (files will be copied in this order) */
        sortColumn: SortColumn
        /** Current sort order on source pane */
        sortOrder: SortOrder
        /** Preview scan ID from CopyDialog (for reusing scan results, optional) */
        previewId: string | null
        onComplete: (filesProcessed: number, bytesProcessed: number) => void
        onCancelled: (filesProcessed: number) => void
        onError: (error: string) => void
    }

    const {
        sourcePaths,
        sourceFolderPath,
        destinationPath,
        direction,
        sortColumn,
        sortOrder,
        previewId,
        onComplete,
        onCancelled,
        onError,
    }: Props = $props()

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

    // Dialog dragging state
    let overlayElement: HTMLDivElement | undefined = $state()
    let dialogPosition = $state({ x: 0, y: 0 })
    let isDragging = $state(false)

    // Progress stages for visualization
    const stages: { id: WriteOperationPhase; label: string }[] = [
        { id: 'scanning', label: 'Scanning' },
        { id: 'copying', label: 'Copying' },
    ]

    function getStageStatus(stageId: WriteOperationPhase): 'done' | 'active' | 'pending' {
        const currentIndex = stages.findIndex((s) => s.id === phase)
        const stageIndex = stages.findIndex((s) => s.id === stageId)

        if (stageIndex < currentIndex) return 'done'
        if (stageIndex === currentIndex) return 'active'
        return 'pending'
    }

    function handleProgress(event: WriteProgressEvent) {
        // Filter by operationId (events are global)
        // If operationId is null, accept the event and capture the ID (handles race condition
        // where events arrive before copyFiles() returns the operationId to the frontend)
        if (operationId === null) {
            operationId = event.operationId
            log.debug('Captured operationId from event: {operationId}', { operationId })
        } else if (event.operationId !== operationId) {
            return
        }

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
        // Filter by operationId (events are global)
        // Accept if operationId is null (race condition) or matches
        if (operationId === null) {
            operationId = event.operationId
        } else if (event.operationId !== operationId) {
            return
        }

        log.info('Copy complete: {filesProcessed} files, {bytesProcessed} bytes', {
            filesProcessed: event.filesProcessed,
            bytesProcessed: event.bytesProcessed,
        })

        cleanup()
        onComplete(event.filesProcessed, event.bytesProcessed)
    }

    /** Converts a WriteOperationError to a user-friendly message. */
    function formatErrorMessage(error: WriteOperationError): string {
        switch (error.type) {
            case 'source_not_found':
                return `Source not found: ${error.path}`
            case 'destination_exists':
                return `Destination already exists: ${error.path}`
            case 'permission_denied':
                return `Permission denied: ${error.path}${error.message ? ` - ${error.message}` : ''}`
            case 'insufficient_space':
                return `Not enough space: need ${formatBytes(error.required)}, only ${formatBytes(error.available)} available`
            case 'same_location':
                return `Source and destination are the same: ${error.path}`
            case 'destination_inside_source':
                return `Can't copy a folder into itself`
            case 'symlink_loop':
                return `Symbolic link loop detected: ${error.path}`
            case 'cancelled':
                return `Operation cancelled: ${error.message}`
            case 'io_error':
                return `I/O error at ${error.path}: ${error.message}`
            default:
                return 'An unknown error occurred'
        }
    }

    function handleError(event: WriteErrorEvent) {
        // Filter by operationId (events are global)
        // Accept if operationId is null (race condition) or matches
        if (operationId === null) {
            operationId = event.operationId
        } else if (event.operationId !== operationId) {
            return
        }

        log.error('Copy error: {errorType}', { errorType: event.error.type, error: event.error })

        cleanup()
        onError(formatErrorMessage(event.error))
    }

    function handleCancelled(event: WriteCancelledEvent) {
        // Filter by operationId (events are global)
        // Accept if operationId is null (race condition) or matches
        if (operationId === null) {
            operationId = event.operationId
        } else if (event.operationId !== operationId) {
            return
        }

        log.info('Copy cancelled after {filesProcessed} files, rolledBack={rolledBack}', {
            filesProcessed: event.filesProcessed,
            rolledBack: event.rolledBack,
        })

        cleanup()
        onCancelled(event.filesProcessed)
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
        log.info('Starting copy operation: {sourceCount} sources to {destination}', {
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

        log.debug('Event subscriptions ready, starting copyFiles')

        try {
            const result = await copyFiles(sourcePaths, destinationPath, {
                conflictResolution: 'stop',
                progressIntervalMs: 100,
                sortColumn,
                sortOrder,
                previewId,
            })

            operationId = result.operationId
            log.info('Copy operation started with operationId: {operationId}', { operationId })
        } catch (err) {
            log.error('Failed to start copy operation: {error}', { error: err })
            cleanup()
            onError(`Failed to start copy: ${String(err)}`)
        }
    }

    async function handleCancel(rollback: boolean) {
        if (!operationId) {
            log.warn('Cancel requested but no operationId yet')
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
        event.stopPropagation()
        if (event.key === 'Escape') {
            // Escape key cancels without rollback (keeps partial files)
            void handleCancel(false)
        }
    }

    // Drag handling for movable dialog
    function handleTitleMouseDown(event: MouseEvent) {
        if ((event.target as HTMLElement).tagName === 'BUTTON') return

        event.preventDefault()
        isDragging = true

        const startX = event.clientX - dialogPosition.x
        const startY = event.clientY - dialogPosition.y

        const handleMouseMove = (e: MouseEvent) => {
            dialogPosition = {
                x: e.clientX - startX,
                y: e.clientY - startY,
            }
        }

        const handleMouseUp = () => {
            isDragging = false
            document.removeEventListener('mousemove', handleMouseMove)
            document.removeEventListener('mouseup', handleMouseUp)
            document.body.style.cursor = ''
        }

        document.addEventListener('mousemove', handleMouseMove)
        document.addEventListener('mouseup', handleMouseUp)
        document.body.style.cursor = 'move'
    }

    onMount(async () => {
        await tick()
        overlayElement?.focus()
        void startOperation()
    })

    onDestroy(() => {
        cleanup()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="progress-dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div
        class="progress-dialog"
        class:dragging={isDragging}
        style="transform: translate({dialogPosition.x}px, {dialogPosition.y}px)"
    >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="dialog-title-bar" onmousedown={handleTitleMouseDown}>
            <h2 id="progress-dialog-title">
                {#if isRollingBack}
                    Rolling back...
                {:else}
                    Copying...
                {/if}
            </h2>
        </div>

        {#if isRollingBack}
            <!-- Rollback in progress -->
            <div class="rollback-section">
                <div class="rollback-indicator">
                    <span class="spinner"></span>
                </div>
                <p class="rollback-message">Deleting {filesDone} copied files...</p>
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
                                <span class="spinner"></span>
                            {:else}
                                <span class="dot"></span>
                            {/if}
                        </div>
                        <span class="stage-label">{stage.label}</span>
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
                <div class="current-file" title={currentFile}>
                    {currentFile}
                </div>
            {/if}

            <!-- Action buttons -->
            <div class="button-row">
                <button
                    class="secondary"
                    onclick={() => handleCancel(false)}
                    disabled={isCancelling}
                    title="Cancel and keep progress"
                >
                    Cancel
                </button>
                <button
                    class="danger"
                    onclick={() => handleCancel(true)}
                    disabled={isCancelling}
                    title="Cancel and delete any partial target files created"
                >
                    Rollback
                </button>
            </div>
        {/if}
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }

    .progress-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        min-width: 420px;
        max-width: 500px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        position: relative;
    }

    .progress-dialog.dragging {
        cursor: move;
    }

    .dialog-title-bar {
        padding: 16px 24px 8px;
        cursor: move;
        user-select: none;
    }

    h2 {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }

    /* Progress stages */
    .progress-stages {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 12px 24px;
        gap: 8px;
    }

    .stage {
        display: flex;
        align-items: center;
        gap: 6px;
        color: var(--color-text-muted);
        font-size: 12px;
        transition: color 0.2s ease;
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
        font-size: 14px;
        font-weight: bold;
    }

    .dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: var(--color-text-muted);
    }

    .spinner {
        width: 14px;
        height: 14px;
        border: 2px solid var(--color-accent);
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .stage-connector {
        width: 24px;
        height: 2px;
        background: var(--color-border-primary);
        transition: background 0.2s ease;
    }

    .stage-connector.done {
        background: var(--color-allow);
    }

    /* Progress bar */
    .progress-section {
        padding: 0 24px;
        margin-bottom: 12px;
    }

    .progress-bar-container {
        width: 100%;
        height: 8px;
        background: var(--color-bg-tertiary);
        border-radius: 4px;
        overflow: hidden;
    }

    .progress-bar {
        height: 100%;
        background: var(--color-accent);
        border-radius: 4px;
        transition: width 0.1s ease-out;
    }

    .progress-info {
        display: flex;
        justify-content: space-between;
        margin-top: 6px;
        font-size: 12px;
    }

    .progress-percent {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .eta {
        color: var(--color-text-muted);
    }

    /* Stats */
    .stats-section {
        padding: 0 24px;
        margin-bottom: 12px;
    }

    .stat-row {
        display: flex;
        justify-content: space-between;
        font-size: 12px;
        padding: 2px 0;
    }

    .stat-label {
        color: var(--color-text-muted);
    }

    .stat-value {
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    /* Current file */
    .current-file {
        padding: 8px 24px;
        font-size: 11px;
        color: var(--color-text-muted);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        margin: 0 16px;
        border-radius: 4px;
    }

    /* Buttons */
    .button-row {
        display: flex;
        gap: 12px;
        justify-content: center;
        padding: 16px 24px 20px;
    }

    button {
        padding: 8px 20px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        min-width: 80px;
    }

    button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .danger {
        background: transparent;
        color: var(--color-error);
        border: 1px solid var(--color-error);
    }

    .danger:hover:not(:disabled) {
        background: var(--color-error);
        color: var(--color-text-primary);
    }

    /* Rollback section */
    .rollback-section {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        padding: 32px 24px;
        gap: 16px;
    }

    .rollback-indicator {
        width: 32px;
        height: 32px;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .rollback-indicator .spinner {
        width: 24px;
        height: 24px;
        border: 3px solid var(--color-error);
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    .rollback-message {
        margin: 0;
        font-size: 13px;
        color: var(--color-text-secondary);
        text-align: center;
    }
</style>
