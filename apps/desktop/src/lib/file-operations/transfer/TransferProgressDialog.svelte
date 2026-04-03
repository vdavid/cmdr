<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import {
        copyBetweenVolumes,
        moveBetweenVolumes,
        moveFiles,
        deleteFiles,
        trashFiles,
        onWriteProgress,
        onWriteComplete,
        onWriteError,
        onWriteCancelled,
        onWriteConflict,
        resolveWriteConflict,
        cancelWriteOperation,
        cancelScanPreview,
        checkScanPreviewStatus,
        onScanPreviewProgress,
        onScanPreviewComplete,
        onScanPreviewError,
        onScanPreviewCancelled,
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
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
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
        /** Per-item sizes for trash progress (from scan or drive index, optional) */
        itemSizes?: number[]
        /** Whether the scan preview is still running (this dialog should subscribe to scan events) */
        scanInProgress?: boolean
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
        itemSizes,
        scanInProgress = false,
        onComplete,
        onCancelled,
        onError,
    }: Props = $props()

    const operationLabelMap: Record<TransferOperationType, string> = {
        copy: 'Copy',
        move: 'Move',
        delete: 'Delete',
        trash: 'Trash',
    }
    const operationGerundMap: Record<TransferOperationType, string> = {
        copy: 'Copying',
        move: 'Moving',
        delete: 'Deleting',
        trash: 'Moving to trash',
    }
    const operationLabel = $derived(operationLabelMap[operationType])
    const operationGerund = $derived(operationGerundMap[operationType])
    const isDeleteOrTrash = $derived(operationType === 'delete' || operationType === 'trash')
    const isCopy = $derived(operationType === 'copy')
    const isMove = $derived(operationType === 'move')

    /** Whether this move involves a non-local volume (MTP, etc.) — backend handles all strategy. */
    const isVolumeMove = $derived(
        operationType === 'move' && (sourceVolumeId !== DEFAULT_VOLUME_ID || (destVolumeId ?? DEFAULT_VOLUME_ID) !== DEFAULT_VOLUME_ID),
    )

    /** Minimum display time (ms) to prevent jarring one-frame flash. */
    const MIN_DISPLAY_MS = 400

    // Scan waiting state (when scan preview is still running from TransferDialog)
    let waitingForScan = $state(false)
    let scanFilesFound = $state(0)
    let scanDirsFound = $state(0)
    let scanBytesFound = $state(0)
    let scanUnlisteners: UnlistenFn[] = []

    // Operation state
    let operationId = $state<string | null>(null)
    let phase = $state<WriteOperationPhase>('scanning')
    let currentFile = $state<string | null>(null)
    let filesDone = $state(0)
    let filesTotal = $state(0)
    let bytesDone = $state(0)
    let bytesTotal = $state(0)
    let startTime = $state(0)
    /** When the active phase (copying/deleting) started, for accurate speed calculation. */
    let activePhaseStartTime = 0
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

    // Sliding window speed samples for blended ETA calculation
    let progressSamples: { timestamp: number; bytesDone: number }[] = []

    const SPEED_WINDOW_MS = 10_000

    /** Blended speed: 90% recent (10s window) + 10% overall average. */
    function getBlendedSpeed(): number {
        if (activePhaseStartTime === 0) return 0

        const now = Date.now()
        const elapsedSeconds = (now - activePhaseStartTime) / 1000
        const overallSpeed = elapsedSeconds > 0 ? bytesDone / elapsedSeconds : 0

        // Discard samples older than the window
        const cutoff = now - SPEED_WINDOW_MS
        progressSamples = progressSamples.filter((s) => s.timestamp >= cutoff)

        if (progressSamples.length < 2) return overallSpeed

        const oldest = progressSamples[0]
        const newest = progressSamples[progressSamples.length - 1]
        const windowSeconds = (newest.timestamp - oldest.timestamp) / 1000
        if (windowSeconds <= 0) return overallSpeed

        const recentSpeed = (newest.bytesDone - oldest.bytesDone) / windowSeconds
        return 0.9 * recentSpeed + 0.1 * overallSpeed
    }

    // Speed and ETA calculation (both use the same blended speed)
    const stats = $derived.by(() => {
        if (startTime === 0) {
            return { bytesPerSecond: 0, estimatedSecondsRemaining: null }
        }
        const bytesPerSecond = getBlendedSpeed()
        const bytesRemaining = bytesTotal - bytesDone
        const estimatedSecondsRemaining = bytesPerSecond > 0 ? bytesRemaining / bytesPerSecond : null
        return { bytesPerSecond, estimatedSecondsRemaining }
    })

    // Progress stages for visualization — the active phase label adapts to operation type.
    const activePhaseId = $derived<WriteOperationPhase>(
        operationType === 'delete' ? 'deleting' : operationType === 'trash' ? 'trashing' : 'copying',
    )
    const stages = $derived<{ id: WriteOperationPhase; label: string }[]>([
        { id: 'scanning', label: 'Scanning' },
        { id: activePhaseId, label: operationGerund },
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

        // Reset speed samples on phase transition (scanning → copying resets bytesDone to 0,
        // which would create negative speed if old scanning samples remain in the window)
        if (event.phase !== phase) {
            progressSamples = []
            activePhaseStartTime = Date.now()
        }

        phase = event.phase
        currentFile = event.currentFile
        filesDone = event.filesDone
        filesTotal = event.filesTotal
        bytesDone = event.bytesDone
        bytesTotal = event.bytesTotal

        // Only collect speed samples during active phases (not scanning)
        if (event.phase !== 'scanning') {
            progressSamples.push({ timestamp: Date.now(), bytesDone: event.bytesDone })
        }
    }

    function handleComplete(event: WriteCompleteEvent) {
        if (!filterEvent({ type: 'complete', event })) return

        log.info('{op} complete: {filesProcessed} files, {bytesProcessed} bytes', {
            op: operationLabel,
            filesProcessed: event.filesProcessed,
            bytesProcessed: event.bytesProcessed,
        })

        cleanup()

        const totalFiles = event.filesProcessed
        const totalBytes = event.bytesProcessed

        // Enforce minimum display time to prevent jarring one-frame flash
        const elapsed = Date.now() - startTime
        const delay = Math.max(0, MIN_DISPLAY_MS - elapsed)
        if (delay > 0) {
            setTimeout(() => {
                onComplete(totalFiles, totalBytes)
            }, delay)
        } else {
            onComplete(totalFiles, totalBytes)
        }
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

    /** Dispatches the backend command based on operation type. */
    async function dispatchOperation(): Promise<{ operationId: string }> {
        const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
        const maxConflictsToShow = getSetting('fileOperations.maxConflictsToShow')

        if (operationType === 'trash') {
            return trashFiles(sourcePaths, itemSizes, { progressIntervalMs, previewId })
        }
        if (operationType === 'delete') {
            return deleteFiles(sourcePaths, { progressIntervalMs, sortColumn, sortOrder, previewId }, sourceVolumeId)
        }
        if (operationType === 'move') {
            // Volume move (MTP or other non-local) — backend handles same-volume, cross-volume, etc.
            if (isVolumeMove) {
                return moveBetweenVolumes(
                    sourceVolumeId,
                    sourcePaths,
                    destVolumeId ?? DEFAULT_VOLUME_ID,
                    destinationPath ?? '',
                    {
                        conflictResolution,
                        progressIntervalMs,
                        maxConflictsToShow,
                        previewId,
                    },
                )
            }
            // Local-to-local move
            return moveFiles(sourcePaths, destinationPath ?? '', {
                conflictResolution,
                progressIntervalMs,
                maxConflictsToShow,
                sortColumn,
                sortOrder,
                previewId,
            })
        }
        // Copy: always use copyBetweenVolumes — the backend handles local-to-local optimization
        return copyBetweenVolumes(
            sourceVolumeId,
            sourcePaths,
            destVolumeId ?? DEFAULT_VOLUME_ID,
            destinationPath ?? '',
            {
                conflictResolution,
                progressIntervalMs,
                maxConflictsToShow,
                previewId,
            },
        )
    }

    async function startOperation() {
        log.info('Starting {op} operation: {sourceCount} sources', {
            op: operationType,
            sourceCount: sourcePaths.length,
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
            const result = await dispatchOperation()

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
        // If still waiting for scan preview, cancel the scan and close
        if (waitingForScan && previewId) {
            log.info('Cancelling scan preview during wait: previewId={previewId}', { previewId })
            void cancelScanPreview(previewId)
            waitingForScan = false
            cleanupScanListeners()
            onCancelled(0)
            return
        }

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

    /** Cleans up scan preview event listeners. */
    function cleanupScanListeners() {
        for (const unlisten of scanUnlisteners) {
            unlisten()
        }
        scanUnlisteners = []
    }

    /**
     * Waits for the scan preview to complete, then starts the write operation.
     * Subscribes to scan events to show progress while waiting.
     */
    async function waitForScanThenStart() {
        if (!previewId) {
            // No preview ID — fall back to normal operation (will do its own scan)
            void startOperation()
            return
        }

        // Check if the scan already completed (race condition: scan finished between
        // TransferDialog closing and this dialog mounting)
        const alreadyComplete = await checkScanPreviewStatus(previewId)
        if (alreadyComplete) {
            log.info('Scan preview already complete for previewId={previewId}, starting operation immediately', {
                previewId,
            })
            void startOperation()
            return
        }

        // Scan is still running — subscribe to events and show progress
        log.info('Scan preview still running for previewId={previewId}, subscribing to events', { previewId })
        waitingForScan = true

        scanUnlisteners.push(
            await onScanPreviewProgress((event) => {
                if (event.previewId !== previewId) return
                scanFilesFound = event.filesFound
                scanDirsFound = event.dirsFound
                scanBytesFound = event.bytesFound
            }),
        )

        scanUnlisteners.push(
            await onScanPreviewComplete((event) => {
                if (event.previewId !== previewId) return
                log.info('Scan preview complete: {filesTotal} files, {bytesTotal} bytes', {
                    filesTotal: event.filesTotal,
                    bytesTotal: event.bytesTotal,
                })
                scanFilesFound = event.filesTotal
                scanDirsFound = event.dirsTotal
                scanBytesFound = event.bytesTotal
                waitingForScan = false
                cleanupScanListeners()
                // Scan results are now cached — start the operation (guaranteed cache hit)
                void startOperation()
            }),
        )

        scanUnlisteners.push(
            await onScanPreviewError((event) => {
                if (event.previewId !== previewId) return
                log.error('Scan preview error: {message}', { message: event.message })
                waitingForScan = false
                cleanupScanListeners()
                onError({
                    type: 'io_error',
                    path: sourcePaths[0] ?? '',
                    message: `Scan failed: ${event.message}`,
                })
            }),
        )

        scanUnlisteners.push(
            await onScanPreviewCancelled((event) => {
                if (event.previewId !== previewId) return
                log.info('Scan preview cancelled')
                waitingForScan = false
                cleanupScanListeners()
                onCancelled(0)
            }),
        )
    }

    onMount(() => {
        if (scanInProgress && previewId) {
            void waitForScanThenStart()
        } else {
            void startOperation()
        }
    })

    onDestroy(() => {
        destroyed = true
        // Cancel scan preview if still waiting for it
        if (waitingForScan && previewId) {
            void cancelScanPreview(previewId)
        }
        cleanupScanListeners()
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
        {#if waitingForScan}
            Scanning...
        {:else if isRollingBack}
            Rolling back...
        {:else if conflictEvent}
            File already exists
        {:else}
            {operationGerund}...
        {/if}
    {/snippet}

    {#if waitingForScan}
        <!-- Scan preview in progress (picked up from TransferDialog) -->
        {#if !isDeleteOrTrash && destinationPath && direction}
            <DirectionIndicator sourcePath={sourceFolderPath} {destinationPath} {direction} />
        {/if}

        <div class="scan-wait-section">
            <div class="scan-wait-stats">
                <div class="scan-stat">
                    <span class="scan-value">{formatBytes(scanBytesFound)}</span>
                </div>
                <span class="scan-divider">/</span>
                <div class="scan-stat">
                    <span class="scan-value">{scanFilesFound}</span>
                    <span class="scan-label">{scanFilesFound === 1 ? 'file' : 'files'}</span>
                </div>
                <span class="scan-divider">/</span>
                <div class="scan-stat">
                    <span class="scan-value">{scanDirsFound}</span>
                    <span class="scan-label">{scanDirsFound === 1 ? 'dir' : 'dirs'}</span>
                </div>
                <span class="scan-spinner"></span>
            </div>
        </div>

        <div class="button-row">
            <Button variant="secondary" onclick={() => void handleCancel(false)}>Cancel</Button>
        </div>
    {:else if !isDeleteOrTrash && conflictEvent}
        <!-- Conflict resolution (copy/move only) -->
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
            <p class="conflict-question">Do you want to skip, rename, or overwrite?</p>

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
                        onclick={() => handleConflictResolution('rename', false)}
                        disabled={isResolvingConflict}
                    >
                        Rename
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
                        onclick={() => handleConflictResolution('rename', true)}
                        disabled={isResolvingConflict}
                    >
                        Rename all
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
                {#if isCopy || isMove}
                    <button
                        class="danger-text"
                        onclick={() => handleCancel(true)}
                        disabled={isCancelling || isResolvingConflict}
                    >
                        Rollback
                    </button>
                {:else}
                    <button
                        class="danger-text"
                        onclick={() => handleCancel(false)}
                        disabled={isCancelling || isResolvingConflict}
                    >
                        Cancel
                    </button>
                {/if}
            </div>
        </div>
    {:else if (isCopy || isMove) && isRollingBack}
        <!-- Rollback in progress (copy only) -->
        <div class="rollback-section">
            <div class="rollback-indicator">
                <span class="spinner spinner-md rollback-spinner"></span>
            </div>
            <p class="rollback-message">
                Deleting {filesDone} copied files...
            </p>
        </div>
    {:else}
        <!-- Direction indicator (copy/move only) -->
        {#if !isDeleteOrTrash && destinationPath && direction}
            <DirectionIndicator sourcePath={sourceFolderPath} {destinationPath} {direction} />
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

        <!-- Dual progress bars (hidden during scanning, bars have no data yet) -->
        {#if phase !== 'scanning'}
            <div class="progress-grid">
                {#if bytesTotal > 0}
                    <span class="progress-label">Size</span>
                    <ProgressBar value={bytesDone / bytesTotal} ariaLabel="Size progress" />
                    <span class="progress-detail">
                        {formatBytes(bytesDone)} / {formatBytes(bytesTotal)}
                        ({Math.round((bytesDone / bytesTotal) * 100)}%)
                    </span>
                {/if}

                <span class="progress-label">{operationType === 'trash' ? 'Items' : 'Files'}</span>
                <ProgressBar value={filesTotal > 0 ? filesDone / filesTotal : 0} ariaLabel="File progress" />
                <span class="progress-detail">{filesDone} / {filesTotal}</span>
                <div class="progress-meta">
                    {#if stats.bytesPerSecond > 0}
                        <span class="progress-speed">{formatBytes(stats.bytesPerSecond)}/s</span>
                    {/if}
                    {#if stats.estimatedSecondsRemaining !== null}
                        <span class="progress-eta">~{formatDuration(stats.estimatedSecondsRemaining)} remaining</span>
                    {/if}
                </div>
            </div>
        {/if}

        <!-- Current file -->
        {#if currentFile}
            <div class="current-file" use:tooltip={{ text: currentFile, overflowOnly: true }}>
                {currentFile}
            </div>
        {/if}

        <!-- Action buttons -->
        <div class="button-row">
            <Button variant="secondary" onclick={() => handleCancel(false)} disabled={isCancelling}>Cancel</Button>
            {#if isCopy || isMove}
                <span use:tooltip={'Cancel and delete any partial target files created'}>
                    <Button variant="danger" onclick={() => handleCancel(true)} disabled={isCancelling}>Rollback</Button
                    >
                </span>
            {/if}
        </div>
    {/if}
</ModalDialog>

<style>
    /* Scan wait section (waiting for scan preview from TransferDialog) */
    .scan-wait-section {
        padding: var(--spacing-md) var(--spacing-xl) var(--spacing-lg);
    }

    .scan-wait-stats {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
    }

    .scan-stat {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
    }

    .scan-value {
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
        font-weight: 500;
    }

    .scan-label {
        color: var(--color-text-tertiary);
    }

    .scan-divider {
        color: var(--color-text-tertiary);
    }

    .scan-spinner {
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-accent);
        border-top-color: transparent;
        border-radius: var(--radius-full);
        animation: spin 0.8s linear infinite;
        margin-left: var(--spacing-xs);
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
        padding: var(--spacing-lg) var(--spacing-xl) var(--spacing-xl);
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
        padding: var(--spacing-md) var(--spacing-xl) var(--spacing-xl);
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
        color: var(--color-accent-text);
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
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
    }

    .conflict-buttons-row {
        display: flex;
        gap: var(--spacing-sm);
        justify-content: center;
    }

    .conflict-buttons :global(button) {
        flex: 1;
        max-width: 120px;
    }

    .conflict-cancel {
        display: flex;
        justify-content: center;
        padding-top: var(--spacing-md);
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
