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
        onWriteSettled,
        onWriteConflict,
        resolveWriteConflict,
        cancelWriteOperation,
        cancelScanPreview,
        checkScanPreviewStatus,
        onScanPreviewProgress,
        onScanPreviewComplete,
        onScanPreviewError,
        onScanPreviewCancelled,
        formatDuration,
        formatFilesPerSecond,
        DEFAULT_VOLUME_ID,
        type WriteProgressEvent,
        type WriteCompleteEvent,
        type WriteErrorEvent,
        type WriteCancelledEvent,
        type WriteSettledEvent,
        type WriteConflictEvent,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type {
        TransferOperationType,
        WriteOperationPhase,
        WriteOperationError,
        FriendlyError,
        SortColumn,
        SortOrder,
        ConflictResolution,
    } from '$lib/file-explorer/types'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { formatDate, formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { pluralize } from '$lib/utils/pluralize'
    import Size from '$lib/ui/Size.svelte'
    import { getSetting } from '$lib/settings'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import { deriveTransferLabel } from './transfer-dialog-utils'
    import ScanPhaseBody from './ScanPhaseBody.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { ScanThroughput } from '../scan-throughput'
    import IconTriangleAlert from '~icons/lucide/triangle-alert'

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
        /** Source filenames known to conflict at dest (from TransferDialog's pre-flight scan).
         *  Forwarded to the BE so it can bulk-skip them upfront under `Skip all`. */
        preKnownConflicts?: string[]
        /** Per-item sizes for trash progress (from scan or drive index, optional) */
        itemSizes?: number[]
        /** Whether the scan preview is still running (this dialog should subscribe to scan events) */
        scanInProgress?: boolean
        onComplete: (filesProcessed: number, filesSkipped: number, bytesProcessed: number) => void
        onCancelled: (filesProcessed: number) => void
        onError: (error: WriteOperationError, friendly?: FriendlyError) => void
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

    /** Title for the scanning phase: names the upcoming action so the user
     *  knows why we're walking the tree, not just "scanning for fun". */
    const scanTitleMap: Record<TransferOperationType, string> = {
        copy: 'Verifying before copy...',
        move: 'Verifying before move...',
        delete: 'Counting items to delete...',
        trash: 'Counting items to trash...',
    }
    const scanTitle = $derived(scanTitleMap[operationType])
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

    /** Whether this move involves a non-local volume (MTP, etc.); backend handles all strategy. */
    const isVolumeMove = $derived(
        operationType === 'move' && (sourceVolumeId !== DEFAULT_VOLUME_ID || (destVolumeId ?? DEFAULT_VOLUME_ID) !== DEFAULT_VOLUME_ID),
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

    const ROLLBACK_UNAVAILABLE_TOOLTIP = 'Rollback is not available for same-volume moves'

    /** Minimum display time (ms) to prevent jarring one-frame flash. */
    const MIN_DISPLAY_MS = 400
    /** After this many ms of waiting for the backend to settle, the
     *  "Cancelling…" label gets a clarifying tail ("(finishing USB transfers)").
     *  Picked at 200 ms so a fast settle (the common case once cancel
     *  propagation lands on the backend) clears before the label ever changes. */
    const SLOW_SETTLE_LABEL_MS = 200
    /** Last-resort cap on how long we'll keep the dialog open after the user
     *  clicks Cancel. The settle gate is supposed to fire `write-cancelled`
     *  + `write-settled` quickly, but if the BE op state was already gone when
     *  we issued the cancel (e.g. it was cleaned up by `cancel_all_write_operations`
     *  during a hot-reload or by a previous teardown), no events ever fire and
     *  the dialog would otherwise stay at "Cancelling…" forever. Ten seconds is
     *  well above the legitimate settle window (USB tear-down is typically < 2 s
     *  even on bad devices) and well below "the user thinks the app is wedged."
     *  See the comment on `handleCancel` for the cases this catches. */
    const CANCEL_SETTLE_FALLBACK_MS = 10_000

    // Scan waiting state (when scan preview is still running from TransferDialog)
    let waitingForScan = $state(false)
    let scanFilesFound = $state(0)
    let scanDirsFound = $state(0)
    let scanBytesFound = $state(0)
    let scanCurrentDir = $state<string | null>(null)
    let scanUnlisteners: UnlistenFn[] = []
    const scanThroughput = new ScanThroughput()
    let scanFilesPerSec = $state<number | null>(null)
    let scanBytesPerSec = $state<number | null>(null)

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
    /** Set when `write-cancelled` arrives for this op. The dialog stays in
     *  "Cancelling…" until BOTH this AND `settleEventReceived` are true,
     *  giving the BE time to tear down in-flight USB / network ops. */
    let cancelEventReceived = $state(false)
    /** Set when `write-settled` arrives for this op. See § "Settle contract"
     *  in `src-tauri/src/file_system/write_operations/CLAUDE.md`. */
    let settleEventReceived = $state(false)
    /** Cached `WriteCancelledEvent` payload — held until settle arrives so we
     *  can pass `filesProcessed` to `onCancelled` at close time. */
    let cancelEventPayload: WriteCancelledEvent | null = null
    /** Flips true once the settle wait has exceeded `SLOW_SETTLE_LABEL_MS`.
     *  Drives the "(finishing USB transfers)" tail on the dialog label. */
    let settleSlow = $state(false)
    let slowSettleTimer: ReturnType<typeof setTimeout> | null = null
    /** Last-resort fallback that closes the dialog if neither `write-cancelled`
     *  nor `write-settled` arrives after the user clicks Cancel. See the
     *  doc comment on `CANCEL_SETTLE_FALLBACK_MS`. */
    let cancelSettleFallbackTimer: ReturnType<typeof setTimeout> | null = null
    /** Set when the operation reaches a terminal state (complete, error, cancel, rollback).
     *  Prevents onDestroy's safety-net cancel from interfering with an already-handled outcome.
     *  Reactive ($state) so the Cancel/Rollback buttons can disable themselves during the
     *  MIN_DISPLAY_MS hold-open window after write-complete; clicking them then would be a
     *  no-op since the backend state is already gone. */
    let operationSettled = $state(false)

    // Events that arrived before we know our operationId (from the command response).
    // Without buffering, a stale event from a previous operation could claim the ID slot first.
    type BufferedEvent =
        | { type: 'progress'; event: WriteProgressEvent }
        | { type: 'complete'; event: WriteCompleteEvent }
        | { type: 'error'; event: WriteErrorEvent }
        | { type: 'cancelled'; event: WriteCancelledEvent }
        | { type: 'settled'; event: WriteSettledEvent }
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
                case 'settled':
                    handleSettled(entry.event)
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

    // Rates + ETA come from the backend (`EtaEstimator` in
    // `write_operations/eta.rs`). The FE just renders them. Null until the
    // backend's warm-up window is over (≈800 ms after a phase change).
    let bytesPerSecond = $state<number | null>(null)
    let filesPerSecond = $state<number | null>(null)
    /** Raw ETA from the backend (`max(ETA_bytes, ETA_files)`). */
    let etaSecondsRaw = $state<number | null>(null)
    /** Display-smoothed ETA: slow EWMA over the raw value to kill flicker on
     *  the "Ns remaining" readout. The estimator itself stays responsive. */
    let etaSecondsDisplay = $state<number | null>(null)

    /** Smooth the displayed ETA toward the latest backend value. Display-only;
     *  the underlying estimator is unsmoothed and reacts to real changes. */
    function updateDisplayEta(raw: number | null) {
        if (raw === null) {
            etaSecondsDisplay = null
            return
        }
        if (etaSecondsDisplay === null) {
            etaSecondsDisplay = raw
            return
        }
        // Cap the change per tick at 25% of the gap. Real changes still
        // propagate quickly (4 ticks ≈ 80 ms × 4 = under a second), while
        // single-tick jitter is dampened.
        etaSecondsDisplay = etaSecondsDisplay + 0.25 * (raw - etaSecondsDisplay)
    }

    // Progress stages for visualization; the active phase label adapts to operation type.
    const activePhaseId = $derived<WriteOperationPhase>(
        operationType === 'delete' ? 'deleting' : operationType === 'trash' ? 'trashing' : 'copying',
    )
    const stages = $derived<{ id: WriteOperationPhase; label: string }[]>([
        { id: 'scanning', label: 'Scanning' },
        { id: activePhaseId, label: operationGerund },
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

    function handleProgress(event: WriteProgressEvent) {
        if (!filterEvent({ type: 'progress', event })) return

        log.debug('Progress event: {phase} {filesDone}/{filesTotal} {filesNoun}, {bytesDone}/{bytesTotal} {bytesNoun}', {
            phase: event.phase,
            filesDone: event.filesDone,
            filesTotal: event.filesTotal,
            filesNoun: pluralize(event.filesTotal, 'file'),
            bytesDone: event.bytesDone,
            bytesTotal: event.bytesTotal,
            bytesNoun: pluralize(event.bytesTotal, 'byte'),
        })

        // Drop the smoothed ETA on phase transitions; the backend estimator
        // resets, so the FE display number should re-warm with it.
        if (event.phase !== phase) {
            etaSecondsDisplay = null
            // Drop the scan throughput history when leaving the scanning phase
            // so a stale sample can't leak into the active phase readout.
            if (phase === 'scanning' && event.phase !== 'scanning') {
                scanThroughput.reset()
                scanFilesPerSec = null
                scanBytesPerSec = null
            }
        }

        // When entering rolling_back phase, set isRollingBack from the backend event
        if (event.phase === 'rolling_back' && !isRollingBack) {
            isRollingBack = true
        }

        phase = event.phase
        currentFile = event.currentFile
        filesDone = event.filesDone
        filesTotal = event.filesTotal
        bytesDone = event.bytesDone
        bytesTotal = event.bytesTotal
        bytesPerSecond = event.bytesPerSecond
        filesPerSecond = event.filesPerSecond
        etaSecondsRaw = event.etaSeconds
        updateDisplayEta(etaSecondsRaw)

        // Scanning-phase metadata (current dir, dirs tally, index-derived
        // expected totals, FE-computed throughput). Mirrors the waitingForScan
        // path so the same scan-phase UI surfaces during the backend's
        // foolproof re-scan.
        if (event.phase === 'scanning') {
            scanFilesFound = event.filesDone
            scanDirsFound = event.dirsDone
            scanBytesFound = event.bytesDone
            scanCurrentDir = event.currentDir ?? null
            const r = scanThroughput.push({
                timestampMs: Date.now(),
                files: event.filesDone,
                bytes: event.bytesDone,
            })
            scanFilesPerSec = r.filesPerSecond
            scanBytesPerSec = r.bytesPerSecond
        }
    }

    function handleComplete(event: WriteCompleteEvent) {
        if (!filterEvent({ type: 'complete', event })) return

        log.info('{op} complete: {filesProcessed} {filesNoun}, {bytesProcessed} {bytesNoun}', {
            op: operationLabel,
            filesProcessed: event.filesProcessed,
            filesNoun: pluralize(event.filesProcessed, 'file'),
            bytesProcessed: event.bytesProcessed,
            bytesNoun: pluralize(event.bytesProcessed, 'byte'),
        })

        operationSettled = true
        cleanup()

        const totalFiles = event.filesProcessed
        const totalSkipped = event.filesSkipped
        const totalBytes = event.bytesProcessed

        // Enforce minimum display time to prevent jarring one-frame flash
        const elapsed = Date.now() - startTime
        const delay = Math.max(0, MIN_DISPLAY_MS - elapsed)
        if (delay > 0) {
            setTimeout(() => {
                onComplete(totalFiles, totalSkipped, totalBytes)
            }, delay)
        } else {
            onComplete(totalFiles, totalSkipped, totalBytes)
        }
    }

    function handleError(event: WriteErrorEvent) {
        if (!filterEvent({ type: 'error', event })) return

        log.error('{op} error: {errorType}', { op: operationLabel, errorType: event.error.type, error: event.error })

        operationSettled = true
        cleanup()
        onError(event.error, event.friendly)
    }

    function handleCancelled(event: WriteCancelledEvent) {
        if (!filterEvent({ type: 'cancelled', event })) return

        log.info('{op} cancelled after {filesProcessed} {filesNoun}, rolledBack={rolledBack}', {
            op: operationLabel,
            filesProcessed: event.filesProcessed,
            filesNoun: pluralize(event.filesProcessed, 'file'),
            rolledBack: event.rolledBack,
        })

        cancelEventReceived = true
        cancelEventPayload = event
        // Mark the operation as settled at the dialog state-machine level so
        // Cancel/Rollback buttons disable (the BE state is already gone or
        // about to be). The "Cancelling…" indicator stays on the FE side
        // until `write-settled` lands.
        operationSettled = true

        // Rollback path: the backend already finished tearing down by the
        // time it emits a non-rolled-back cancel (the user clicked Cancel
        // during Rollback) or a rolled-back cancel (rollback completed
        // normally). In both cases, settling has effectively happened. We
        // still wait for `write-settled` for consistency, but start the slow
        // label timer regardless.
        startSlowSettleTimer()

        maybeFinishCancelClose()
    }

    /** Called once the BE has signalled `write-settled` for the operation
     *  the dialog is bound to. Combines with `write-cancelled` (already
     *  received) to drive the close-out. Defensive about ordering: if
     *  settle somehow arrives before cancelled, we wait. */
    function handleSettled(event: WriteSettledEvent) {
        if (!filterEvent({ type: 'settled', event })) return

        log.debug('Settle event arrived for op={operationId}', { operationId: event.operationId })

        settleEventReceived = true
        clearSlowSettleTimer()
        clearCancelSettleFallbackTimer()
        maybeFinishCancelClose()
    }

    function startSlowSettleTimer() {
        if (slowSettleTimer !== null) return
        slowSettleTimer = setTimeout(() => {
            settleSlow = true
            slowSettleTimer = null
        }, SLOW_SETTLE_LABEL_MS)
    }

    function clearSlowSettleTimer() {
        if (slowSettleTimer !== null) {
            clearTimeout(slowSettleTimer)
            slowSettleTimer = null
        }
        settleSlow = false
    }

    function startCancelSettleFallbackTimer() {
        if (cancelSettleFallbackTimer !== null) return
        cancelSettleFallbackTimer = setTimeout(() => {
            cancelSettleFallbackTimer = null
            // If we still haven't received both terminal events, the BE op state
            // is gone (or never existed) — synthesise a clean close so the dialog
            // doesn't stay at "Cancelling…" forever. Hand the FE a Cancelled
            // outcome with `filesProcessed: 0` so the caller's cleanup runs.
            if (!cancelEventReceived || !settleEventReceived) {
                log.warn(
                    'Cancel settle fallback fired for op={operationId} after {ms}ms (cancelled={c}, settled={s}); closing dialog',
                    {
                        operationId,
                        ms: CANCEL_SETTLE_FALLBACK_MS,
                        c: cancelEventReceived,
                        s: settleEventReceived,
                    },
                )
                cleanup()
                onCancelled(0)
            }
        }, CANCEL_SETTLE_FALLBACK_MS)
    }

    function clearCancelSettleFallbackTimer() {
        if (cancelSettleFallbackTimer !== null) {
            clearTimeout(cancelSettleFallbackTimer)
            cancelSettleFallbackTimer = null
        }
    }

    /** Closes the dialog if both `write-cancelled` and `write-settled` have
     *  arrived. Applies the existing `MIN_DISPLAY_MS` floor so a sub-frame
     *  cancel doesn't flash. Idempotent: safe to call from both handlers. */
    function maybeFinishCancelClose() {
        if (!cancelEventReceived || !settleEventReceived) return
        if (cancelEventPayload === null) return

        const payload = cancelEventPayload
        cancelEventPayload = null // single-shot
        cleanup()

        const elapsed = Date.now() - startTime
        const delay = Math.max(0, MIN_DISPLAY_MS - elapsed)
        if (delay > 0) {
            setTimeout(() => {
                onCancelled(payload.filesProcessed)
            }, delay)
        } else {
            onCancelled(payload.filesProcessed)
        }
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
        log.debug('Cleaning up {count} {listenersNoun}', {
            count: unlisteners.length,
            listenersNoun: pluralize(unlisteners.length, 'event listener'),
        })
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
        clearSlowSettleTimer()
        clearCancelSettleFallbackTimer()
    }

    /** Dispatches the backend command based on operation type. */
    async function dispatchOperation(): Promise<{ operationId: string }> {
        const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
        const maxConflictsToShow = getSetting('fileOperations.maxConflictsToShow')

        if (operationType === 'trash') {
            return trashFiles(sourcePaths, itemSizes, { progressIntervalMs, previewId })
        }
        if (operationType === 'delete') {
            return deleteFiles(
                sourcePaths,
                { progressIntervalMs, sortColumn, sortOrder, previewId },
                sourceVolumeId,
            )
        }
        if (operationType === 'move') {
            // Volume move (MTP or other non-local); backend handles same-volume, cross-volume, etc.
            if (isVolumeMove) {
                return moveBetweenVolumes(
                    sourceVolumeId,
                    sourcePaths,
                    destVolumeId ?? DEFAULT_VOLUME_ID,
                    destinationPath ?? '',
                    {
                        conflictResolution: conflictResolution ?? 'stop',
                        progressIntervalMs,
                        maxConflictsToShow,
                        previewId,
                        preKnownConflicts: preKnownConflicts ?? [],
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
                preKnownConflicts: preKnownConflicts ?? [],
            })
        }
        // Copy: always use copyBetweenVolumes; the backend handles local-to-local optimization
        return copyBetweenVolumes(
            sourceVolumeId,
            sourcePaths,
            destVolumeId ?? DEFAULT_VOLUME_ID,
            destinationPath ?? '',
            {
                conflictResolution: conflictResolution ?? 'stop',
                progressIntervalMs,
                maxConflictsToShow,
                previewId,
                preKnownConflicts: preKnownConflicts ?? [],
            },
        )
    }

    async function startOperation() {
        log.info('Starting {op} operation: {sourceCount} {sourcesNoun}', {
            op: operationType,
            sourceCount: sourcePaths.length,
            sourcesNoun: pluralize(sourcePaths.length, 'source'),
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
        unlisteners.push(await onWriteSettled(handleSettled))
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
            // cancel the operation immediately and bail out. Crucially, we must call
            // `onCancelled` so the parent removes the dialog from state — otherwise
            // the listeners are torn down by `cleanup()` but the `<TransferProgressDialog>`
            // stays mounted forever (the BE eventually emits `write-cancelled` +
            // `write-settled`, but no one's listening anymore). That stuck dialog
            // poisons every following operation through ensureAppReady's Escape.
            if (destroyed) {
                log.info('Dialog destroyed before operationId arrived; cancelling op={operationId}', {
                    operationId,
                })
                void cancelWriteOperation(operationId, true)
                cleanup()
                onCancelled(0)
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
            log.warn('Cancel requested but no operationId yet; will cancel after IPC resolves')
            destroyed = true
            return
        }
        if (isCancelling) {
            log.debug('Cancel already in progress')
            return
        }

        if (isRollingBack && !rollback) {
            // Cancel during rollback: stop deleting, keep remaining files
            log.info('Cancelling rollback for operation: {operationId}', { operationId })
            isCancelling = true
            try {
                await cancelWriteOperation(operationId, false)
                log.debug('Rollback cancel request sent successfully')
                // Dialog will close when write-cancelled event is received
            } catch (err) {
                log.error('Failed to cancel rollback: {error}', { error: err })
                isCancelling = false
            }
            return
        }

        if (isRollingBack) {
            log.debug('Rollback already in progress')
            return
        }

        if (rollback) {
            // Rollback: keep dialog open, backend will enter rolling_back phase with progress events
            log.info('Rolling back operation: {operationId}', { operationId })
            operationSettled = true
            isRollingBack = true
            try {
                await cancelWriteOperation(operationId, true)
                log.debug('Rollback request sent successfully')
                // Dialog stays open; progress events with phase=rolling_back will update the UI.
                // Dialog closes when write-cancelled event is received.
            } catch (err) {
                log.error('Failed to rollback operation: {error}', { error: err })
                isRollingBack = false
            }
        } else {
            // Cancel: keep partial files. Stay in "Cancelling…" until both
            // `write-cancelled` and `write-settled` have landed: the BE may
            // still be tearing down USB / network sessions, and dispatching a
            // new op against a volume in that state is what wedged the device
            // in the original incident. See the "Settle contract" in
            // `src-tauri/src/file_system/write_operations/CLAUDE.md`.
            log.info('Cancelling operation (keeping partial files): {operationId}', { operationId })
            isCancelling = true
            startSlowSettleTimer()
            startCancelSettleFallbackTimer()
            try {
                await cancelWriteOperation(operationId, false)
                log.debug('Cancel request sent; waiting for write-cancelled + write-settled')
                // Don't close here. The dialog closes from
                // `maybeFinishCancelClose` once both events arrive.
            } catch (err) {
                log.error('Failed to cancel operation: {error}', { error: err })
                isCancelling = false
                clearSlowSettleTimer()
                clearCancelSettleFallbackTimer()
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

    /** Returns true if the event belongs to our scan preview. */
    function isOurScanEvent(eventPreviewId: string): boolean {
        return eventPreviewId === previewId
    }

    /**
     * Waits for the scan preview to complete, then starts the write operation.
     *
     * Two independent signals can say "scan done": the `scan-preview-complete`
     * event firing, or the post-subscription `checkScanPreviewStatus` IPC
     * returning true. Either can win the race. Both converge on `kickOff()`,
     * which is idempotent via the `started` flag; the operation dispatches
     * exactly once, even if both signals arrive during the `await`.
     *
     * We subscribe to events BEFORE the status check so a fast completion
     * between subscription and check isn't missed.
     *
     * Precondition: previewId must be non-null (guaranteed by TransferDialog,
     * which awaits startScanPreview IPC before calling onConfirm).
     */
    async function waitForScanThenStart() {
        if (!previewId) {
            log.error('waitForScanThenStart called with null previewId; TransferDialog invariant violated')
            void startOperation()
            return
        }

        let started = false
        const kickOff = () => {
            if (started) return
            started = true
            cleanupScanListeners()
            void startOperation()
        }

        // Subscribe to events FIRST to avoid missing fast completions.
        // Same pattern as TransferDialog.startScan().
        scanUnlisteners.push(
            await onScanPreviewProgress((event) => {
                if (!isOurScanEvent(event.previewId)) return
                scanFilesFound = event.filesFound
                scanDirsFound = event.dirsFound
                scanBytesFound = event.bytesFound
                scanCurrentDir = event.currentDir ?? null
                const r = scanThroughput.push({
                    timestampMs: Date.now(),
                    files: event.filesFound,
                    bytes: event.bytesFound,
                })
                scanFilesPerSec = r.filesPerSecond
                scanBytesPerSec = r.bytesPerSecond
            }),
        )

        scanUnlisteners.push(
            await onScanPreviewComplete((event) => {
                if (!isOurScanEvent(event.previewId)) return
                log.info('Scan preview complete: {filesTotal} {filesNoun}, {bytesTotal} {bytesNoun}', {
                    filesTotal: event.filesTotal,
                    filesNoun: pluralize(event.filesTotal, 'file'),
                    bytesTotal: event.bytesTotal,
                    bytesNoun: pluralize(event.bytesTotal, 'byte'),
                })
                scanFilesFound = event.filesTotal
                scanDirsFound = event.dirsTotal
                scanBytesFound = event.bytesTotal
                waitingForScan = false
                kickOff()
            }),
        )

        scanUnlisteners.push(
            await onScanPreviewError((event) => {
                if (!isOurScanEvent(event.previewId)) return
                if (started) return // already dispatched or terminated; ignore late errors
                started = true // terminal; don't let a late scan-complete dispatch an operation
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
                if (!isOurScanEvent(event.previewId)) return
                if (started) return // already dispatched or terminated; ignore late cancellations
                started = true // terminal; don't let a late scan-complete dispatch an operation
                log.info('Scan preview cancelled')
                waitingForScan = false
                cleanupScanListeners()
                onCancelled(0)
            }),
        )

        // NOW check if already complete (covers race where scan finished during subscription setup)
        const alreadyComplete = await checkScanPreviewStatus(previewId)
        if (alreadyComplete) {
            log.info('Scan preview already complete for previewId={previewId}, starting operation immediately', {
                previewId,
            })
            kickOff()
            return
        }

        log.info('Scan preview still running for previewId={previewId}, subscribing to events', { previewId })
        waitingForScan = true
    }

    onMount(() => {
        if (scanInProgress) {
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
        if (operationId && !operationSettled) {
            // Unexpected teardown (hot-reload, navigation, window close): stop the operation
            // but don't roll back; never do silent background work without visual feedback.
            void cancelWriteOperation(operationId, false)
        }
        cleanup()
    })
</script>

<ModalDialog
    titleId="progress-dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-progress"
    onclose={() => {
        void handleCancel(false)
    }}
    containerStyle="width: 500px"
>
    {#snippet title()}
        {#if waitingForScan}
            {scanTitle}
        {:else if isRollingBack}
            Rolling back...
        {:else if isCancelling || cancelEventReceived}
            {#if settleSlow}
                Cancelling... (finishing USB transfers)
            {:else}
                Cancelling...
            {/if}
        {:else if conflictEvent}
            File already exists
        {:else if phase === 'flushing'}
            Writing the last piece...
        {:else}
            {operationGerund}...
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
                    void handleCancel(false)
                }}>Cancel</Button
            >
        </div>
    {:else if !isDeleteOrTrash && conflictEvent}
        <!-- Conflict resolution (copy/move only). The same shape — filename,
             "Existing:" / "New:" rows, the 4×2 button grid, the Rollback row —
             is used for every clash type. Variants only differ in row labels,
             a red warning block above the filename for file→folder, and the
             "Overwrite" button copy in that one case. -->
        {@const fileName = conflictEvent.destinationPath.split('/').pop() ?? ''}
        {@const existingIsNewer = conflictEvent.destinationIsNewer}
        {@const newIsNewer = !existingIsNewer && conflictEvent.sourceModified !== conflictEvent.destinationModified}
        {@const sizeDiff = conflictEvent.sizeDifference}
        {@const existingIsLarger = sizeDiff !== null && sizeDiff > 0}
        {@const newIsLarger = sizeDiff !== null && sizeDiff < 0}
        {@const sourceIsDir = conflictEvent.sourceIsDirectory}
        {@const destIsDir = conflictEvent.destinationIsDirectory}
        {@const isTypeMismatch = sourceIsDir !== destIsDir}
        {@const isFileOverFolder = isTypeMismatch && destIsDir}
        {@const existingLabel = destIsDir ? 'Existing (folder):' : sourceIsDir ? 'Existing (file):' : 'Existing:'}
        {@const newLabel = sourceIsDir ? 'New (folder):' : isFileOverFolder ? 'New (file):' : 'New:'}
        {@const overwriteLabel = isFileOverFolder ? 'Overwrite folder with file' : 'Overwrite'}
        {@const overwriteAllLabel = isFileOverFolder ? 'Overwrite folders with files' : 'Overwrite all'}
        {@const destSize = conflictEvent.destinationSize}
        {@const destSizeUnknown = destSize === null}
        {@const srcSize = conflictEvent.sourceSize}
        {@const srcSizeUnknown = srcSize === null}
        {@const smallerDisabledTooltip = destSizeUnknown
            ? "Can't compare — target folder size is unknown."
            : undefined}
        <div class="conflict-section">
            {#if isFileOverFolder}
                <!-- Red warning sits below the title and above the filename.
                     The "boring" title is `File already exists`; the destructive
                     swap gets called out here so the user can't miss it. -->
                <p class="conflict-warning" role="alert">
                    <span class="conflict-warning-icon" aria-hidden="true">
                        <IconTriangleAlert width="16" height="16" />
                    </span>
                    <span>
                        The target exists and is a <strong>folder</strong>. You're about to overwrite it with a
                        <strong>file</strong> by the same name. All contents of the target folder would be deleted and
                        replaced by the file. What to do?
                    </span>
                </p>
            {/if}

            <!-- Filename -->
            <p class="conflict-filename" use:tooltip={{ text: conflictEvent.destinationPath, overflowOnly: true }}>
                {fileName}
            </p>

            <!-- File comparison: same shape across all variants. Type tags
                 (`Existing (file):` / `New (folder):` etc.) flag the mismatch
                 without breaking the layout. Size renders normally when known
                 and substitutes `(unknown)` in muted color when the BE could
                 not look the destination folder size up. -->
            <div class="conflict-comparison">
                <div class="conflict-file">
                    <span class="conflict-file-label">{existingLabel}</span>
                    {#if destSizeUnknown}
                        <span class="conflict-file-size unknown">(unknown)</span>
                    {:else}
                        <span class="conflict-file-size {getSizeColorClass(destSize)}"
                            >{formatFileSize(destSize)}</span
                        >
                    {/if}
                    {#if existingIsLarger}<span class="conflict-annotation larger">(larger)</span>{/if}
                    <span class="conflict-file-date"
                        >{conflictEvent.destinationModified
                            ? formatDate(conflictEvent.destinationModified)
                            : ''}</span
                    >
                    {#if existingIsNewer}<span class="conflict-annotation newer">(newer)</span>{/if}
                </div>
                <div class="conflict-file">
                    <span class="conflict-file-label">{newLabel}</span>
                    {#if srcSizeUnknown}
                        <span class="conflict-file-size unknown">(unknown)</span>
                    {:else}
                        <span class="conflict-file-size {getSizeColorClass(srcSize)}"
                            >{formatFileSize(srcSize)}</span
                        >
                    {/if}
                    {#if newIsLarger}<span class="conflict-annotation larger">(larger)</span>{/if}
                    <span class="conflict-file-date"
                        >{conflictEvent.sourceModified ? formatDate(conflictEvent.sourceModified) : ''}</span
                    >
                    {#if newIsNewer}<span class="conflict-annotation newer">(newer)</span>{/if}
                </div>
            </div>

            <!-- Buttons. Two columns: left = this-item, right = apply-to-all.
                 Last row holds the conditional bulk variants: `Overwrite all
                 smaller` only works when the destination size is known
                 (a folder dest with no index size disables it with a tooltip);
                 `Overwrite all older` always stays enabled (mtime is always
                 available even for folder destinations). -->
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
                        onclick={() => handleConflictResolution('skip', true)}
                        disabled={isResolvingConflict}
                    >
                        Skip all
                    </Button>
                </div>
                <div class="conflict-buttons-row">
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('rename', false)}
                        disabled={isResolvingConflict}
                    >
                        Rename
                    </Button>
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('rename', true)}
                        disabled={isResolvingConflict}
                    >
                        Rename all
                    </Button>
                </div>
                <div class="conflict-buttons-row">
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('overwrite', false)}
                        disabled={isResolvingConflict}
                    >
                        {overwriteLabel}
                    </Button>
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('overwrite', true)}
                        disabled={isResolvingConflict}
                    >
                        {overwriteAllLabel}
                    </Button>
                </div>
                <div class="conflict-buttons-row">
                    <span use:tooltip={smallerDisabledTooltip} class="conflict-button-wrap">
                        <Button
                            variant="secondary"
                            onclick={() => handleConflictResolution('overwrite_smaller', true)}
                            disabled={isResolvingConflict || destSizeUnknown}
                        >
                            Overwrite all smaller
                        </Button>
                    </span>
                    <Button
                        variant="secondary"
                        onclick={() => handleConflictResolution('overwrite_older', true)}
                        disabled={isResolvingConflict}
                    >
                        Overwrite all older
                    </Button>
                </div>
            </div>

            <!-- Cancel at bottom. Same-volume volume moves have no backend
                 rollback, so Rollback is DISABLED (with a tooltip) and a plain
                 Cancel sits alongside it so the user can always back out. -->
            <div class="conflict-cancel">
                {#if isSameVolumeMove}
                    <button
                        class="danger-text"
                        onclick={() => handleCancel(false)}
                        disabled={isCancelling || isResolvingConflict}
                    >
                        Cancel
                    </button>
                    <span use:tooltip={ROLLBACK_UNAVAILABLE_TOOLTIP} class="disabled-button-wrap">
                        <button class="danger-text" disabled>Rollback</button>
                    </span>
                {:else if isCopy || isMove}
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
                    <span class="progress-label">Size</span>
                    <ProgressBar value={bytesDone / bytesTotal} ariaLabel="Size progress" />
                    <span class="progress-detail">
                        <Size bytes={bytesDone} /> / <Size bytes={bytesTotal} />
                        ({Math.round((bytesDone / bytesTotal) * 100)}%)
                    </span>
                {/if}

                <span class="progress-label">{operationType === 'trash' ? 'Items' : 'Files'}</span>
                <ProgressBar value={filesTotal > 0 ? filesDone / filesTotal : 0} ariaLabel="File progress" />
                <span class="progress-detail">{formatNumber(filesDone)} / {formatNumber(filesTotal)}</span>
                <div class="progress-meta">
                    <span class="progress-speeds">
                        {#if bytesPerSecond !== null && bytesPerSecond > 0}
                            <span class="progress-speed"><Size bytes={bytesPerSecond} />/s</span>
                        {/if}
                        {#if filesPerSecond !== null}
                            {@const filesPerSecLabel = formatFilesPerSecond(filesPerSecond)}
                            {#if filesPerSecLabel !== null}
                                <span class="progress-speed">{filesPerSecLabel}</span>
                            {/if}
                        {/if}
                    </span>
                    {#if etaSecondsDisplay !== null}
                        <span class="progress-eta">~{formatDuration(etaSecondsDisplay)} remaining</span>
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
                This share uses the system connection. Cancel and rollback may be delayed.
            </p>
        {/if}

        <!-- Action buttons -->
        <!-- Once `operationSettled` is true (write-complete / write-cancelled / write-error
             arrived) the backend state is gone, so a Rollback click can't be honored; disable
             both buttons during the MIN_DISPLAY_MS hold-open window. Without this, the user can
             click Rollback after the copy completed and silently get nothing. -->
        <div class="button-row">
            <Button
                variant="secondary"
                onclick={() => handleCancel(false)}
                disabled={isCancelling || operationSettled}>Cancel</Button
            >
            {#if isCopy || isMove}
                {#if isRollingBack}
                    <Button variant="danger" disabled>Rolling back...</Button>
                {:else if isSameVolumeMove}
                    <!-- Same-volume volume moves have no backend rollback; the
                         button is disabled with an explanatory tooltip. Plain
                         Cancel above stays reachable. -->
                    <span use:tooltip={ROLLBACK_UNAVAILABLE_TOOLTIP}>
                        <Button variant="danger" disabled>Rollback</Button>
                    </span>
                {:else}
                    <span use:tooltip={'Cancel and delete any partial target files created'}>
                        <Button
                            variant="danger"
                            onclick={() => handleCancel(true)}
                            disabled={isCancelling || operationSettled}>Rollback</Button
                        >
                    </span>
                {/if}
            {/if}
        </div>
    {/if}
</ModalDialog>

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
        text-align: center;
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: var(--spacing-lg) var(--spacing-xl) var(--spacing-xl);
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

    /* Red warning block for file→folder clashes. Sits below the title and
       above the filename so the user sees the destructive nature before any
       button. Mirrors the warning-callout visual vocabulary used elsewhere
       (icon + sentence in a tinted block) but in red, not yellow, to mark
       the higher destructive stakes. */
    .conflict-warning {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        border: 1px solid var(--color-error-border);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .conflict-warning strong {
        font-weight: 600;
    }

    .conflict-warning-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        color: var(--color-error-text);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- align icon with first line of text */
        margin-top: 1px;
    }

    /* `(unknown)` placeholder used in the Existing-size slot when the BE
       couldn't look up the destination folder's size (no drive-index entry).
       Muted so it reads as "no value" rather than masquerading as a real
       byte-range color. */
    .conflict-file-size.unknown {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    /* Wrap so the tooltip has a host element when the inner Button is
       disabled (disabled buttons don't fire pointer events themselves).
       The inner button still gets `flex: 1` via the existing
       `.conflict-buttons :global(button)` rule below, so this wrap matches
       button-row width. */
    .conflict-button-wrap {
        display: flex;
        flex: 1;
        max-width: 200px;
    }

    .conflict-button-wrap > :global(button) {
        flex: 1;
        max-width: none;
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
        max-width: 200px;
    }

    .conflict-cancel {
        display: flex;
        justify-content: center;
        gap: var(--spacing-md);
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border-strong);
    }

    /* Host for the disabled Rollback button so the tooltip still fires (a
       disabled button swallows its own pointer events). Mirrors
       `.conflict-button-wrap`'s purpose for the smaller-disabled bulk action. */
    .disabled-button-wrap {
        display: inline-flex;
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
