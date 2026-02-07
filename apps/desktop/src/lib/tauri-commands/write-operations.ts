// Copy/move/delete operations + event handlers

import { type Event, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import type {
    ConflictInfo,
    ConflictResolution,
    DryRunResult,
    OperationStatus,
    OperationSummary,
    ScanProgressEvent,
    ScanPreviewCancelledEvent,
    ScanPreviewCompleteEvent,
    ScanPreviewErrorEvent,
    ScanPreviewProgressEvent,
    ScanPreviewStartResult,
    SortColumn,
    SortOrder,
    WriteCancelledEvent,
    WriteCompleteEvent,
    WriteConflictEvent,
    WriteErrorEvent,
    WriteOperationConfig,
    WriteOperationError,
    WriteOperationStartResult,
    WriteProgressEvent,
} from '../file-explorer/types'

export type { Event, UnlistenFn }
export { listen }

// Re-export types for backward compatibility
export type {
    WriteCancelledEvent,
    WriteCompleteEvent,
    WriteConflictEvent,
    WriteErrorEvent,
    WriteOperationConfig,
    WriteOperationError,
    WriteOperationStartResult,
    WriteProgressEvent,
    ConflictInfo,
    DryRunResult,
    OperationStatus,
    OperationSummary,
    ScanProgressEvent,
    ScanPreviewStartResult,
    ScanPreviewProgressEvent,
    ScanPreviewCompleteEvent,
    ScanPreviewErrorEvent,
    ScanPreviewCancelledEvent,
}

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/** Starts scanning source files immediately, emitting progress events for the Copy dialog. */
export async function startScanPreview(
    sources: string[],
    sortColumn: SortColumn,
    sortOrder: SortOrder,
    progressIntervalMs?: number,
): Promise<ScanPreviewStartResult> {
    return invoke<ScanPreviewStartResult>('start_scan_preview', { sources, sortColumn, sortOrder, progressIntervalMs })
}

export async function cancelScanPreview(previewId: string): Promise<void> {
    await invoke('cancel_scan_preview', { previewId })
}

export async function onScanPreviewProgress(callback: (event: ScanPreviewProgressEvent) => void): Promise<UnlistenFn> {
    return listen<ScanPreviewProgressEvent>('scan-preview-progress', (event) => {
        callback(event.payload)
    })
}

export async function onScanPreviewComplete(callback: (event: ScanPreviewCompleteEvent) => void): Promise<UnlistenFn> {
    return listen<ScanPreviewCompleteEvent>('scan-preview-complete', (event) => {
        callback(event.payload)
    })
}

export async function onScanPreviewError(callback: (event: ScanPreviewErrorEvent) => void): Promise<UnlistenFn> {
    return listen<ScanPreviewErrorEvent>('scan-preview-error', (event) => {
        callback(event.payload)
    })
}

export async function onScanPreviewCancelled(
    callback: (event: ScanPreviewCancelledEvent) => void,
): Promise<UnlistenFn> {
    return listen<ScanPreviewCancelledEvent>('scan-preview-cancelled', (event) => {
        callback(event.payload)
    })
}

// ============================================================================
// Write operations (copy, move, delete)
// ============================================================================

/** Emits write-progress, write-complete, write-error, write-cancelled events. */
export async function copyFiles(
    sources: string[],
    destination: string,
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('copy_files', { sources, destination, config: config ?? {} })
}

/** Uses instant rename for same-filesystem, copy+delete for cross-filesystem. Same events as copyFiles. */
export async function moveFiles(
    sources: string[],
    destination: string,
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('move_files', { sources, destination, config: config ?? {} })
}

/** Recursively deletes files and directories. Same events as copyFiles. */
export async function deleteFiles(
    sources: string[],
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('delete_files', { sources, config: config ?? {} })
}

export async function cancelWriteOperation(operationId: string, rollback: boolean): Promise<void> {
    await invoke('cancel_write_operation', { operationId, rollback })
}

/** In Stop mode, the operation pauses on conflict and waits for this call to proceed. */
export async function resolveWriteConflict(
    operationId: string,
    resolution: ConflictResolution,
    applyToAll: boolean,
): Promise<void> {
    await invoke('resolve_write_conflict', { operationId, resolution, applyToAll })
}

export async function listActiveOperations(): Promise<OperationSummary[]> {
    return invoke<OperationSummary[]>('list_active_operations')
}

export async function getOperationStatus(operationId: string): Promise<OperationStatus | null> {
    return invoke<OperationStatus | null>('get_operation_status', { operationId })
}

export function isWriteOperationError(error: unknown): error is WriteOperationError {
    return (
        typeof error === 'object' &&
        error !== null &&
        'type' in error &&
        typeof (error as { type: unknown }).type === 'string'
    )
}

// ============================================================================
// Write operation event helpers
// ============================================================================

export async function onWriteProgress(callback: (event: WriteProgressEvent) => void): Promise<UnlistenFn> {
    return listen<WriteProgressEvent>('write-progress', (event) => {
        callback(event.payload)
    })
}

export async function onWriteComplete(callback: (event: WriteCompleteEvent) => void): Promise<UnlistenFn> {
    return listen<WriteCompleteEvent>('write-complete', (event) => {
        callback(event.payload)
    })
}

export async function onWriteError(callback: (event: WriteErrorEvent) => void): Promise<UnlistenFn> {
    return listen<WriteErrorEvent>('write-error', (event) => {
        callback(event.payload)
    })
}

export async function onWriteCancelled(callback: (event: WriteCancelledEvent) => void): Promise<UnlistenFn> {
    return listen<WriteCancelledEvent>('write-cancelled', (event) => {
        callback(event.payload)
    })
}

/** Only emitted in Stop conflict resolution mode. */
export async function onWriteConflict(callback: (event: WriteConflictEvent) => void): Promise<UnlistenFn> {
    return listen<WriteConflictEvent>('write-conflict', (event) => {
        callback(event.payload)
    })
}

export async function onScanProgress(callback: (event: ScanProgressEvent) => void): Promise<UnlistenFn> {
    return listen<ScanProgressEvent>('scan-progress', (event) => {
        callback(event.payload)
    })
}

export async function onScanConflict(callback: (event: ConflictInfo) => void): Promise<UnlistenFn> {
    return listen<ConflictInfo>('scan-conflict', (event) => {
        callback(event.payload)
    })
}

export async function onDryRunComplete(callback: (event: DryRunResult) => void): Promise<UnlistenFn> {
    return listen<DryRunResult>('dry-run-complete', (event) => {
        callback(event.payload)
    })
}

// ============================================================================
// Unified write operation event subscription
// ============================================================================

/** Handlers for write operation events. All handlers are optional. */
export interface WriteOperationHandlers {
    onProgress?: (event: WriteProgressEvent) => void
    onComplete?: (event: WriteCompleteEvent) => void
    onError?: (event: WriteErrorEvent) => void
    onCancelled?: (event: WriteCancelledEvent) => void
    onConflict?: (event: WriteConflictEvent) => void
    /** For dry-run mode: progress during scanning */
    onScanProgress?: (event: ScanProgressEvent) => void
    /** For dry-run mode: individual conflicts as they're found */
    onScanConflict?: (event: ConflictInfo) => void
    /** For dry-run mode: final result */
    onDryRunComplete?: (event: DryRunResult) => void
}

/**
 * Subscribes to all events for a specific write operation.
 * Filters events by operationId so handlers only receive events for this operation.
 * Returns a single unlisten function that cleans up all subscriptions.
 *
 * @example
 * ```ts
 * const unlisten = await onOperationEvents(result.operationId, {
 *   onProgress: (e) => updateProgressBar(e.bytesDone / e.bytesTotal),
 *   onComplete: (e) => showSuccess(`Copied ${e.filesProcessed} files`),
 *   onError: (e) => showError(e.error),
 * })
 * // Later: unlisten() to clean up all subscriptions
 * ```
 */
export async function onOperationEvents(operationId: string, handlers: WriteOperationHandlers): Promise<UnlistenFn> {
    const unlisteners: UnlistenFn[] = []

    if (handlers.onProgress) {
        const handler = handlers.onProgress
        unlisteners.push(
            await listen<WriteProgressEvent>('write-progress', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onComplete) {
        const handler = handlers.onComplete
        unlisteners.push(
            await listen<WriteCompleteEvent>('write-complete', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onError) {
        const handler = handlers.onError
        unlisteners.push(
            await listen<WriteErrorEvent>('write-error', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onCancelled) {
        const handler = handlers.onCancelled
        unlisteners.push(
            await listen<WriteCancelledEvent>('write-cancelled', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onConflict) {
        const handler = handlers.onConflict
        unlisteners.push(
            await listen<WriteConflictEvent>('write-conflict', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onScanProgress) {
        const handler = handlers.onScanProgress
        unlisteners.push(
            await listen<ScanProgressEvent>('scan-progress', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    // Note: scan-conflict events don't have operationId, they're streamed during the scan
    // The frontend should only subscribe when doing a dry-run for a specific operation
    if (handlers.onScanConflict) {
        const handler = handlers.onScanConflict
        unlisteners.push(
            await listen<ConflictInfo>('scan-conflict', (event) => {
                handler(event.payload)
            }),
        )
    }

    if (handlers.onDryRunComplete) {
        const handler = handlers.onDryRunComplete
        unlisteners.push(
            await listen<DryRunResult>('dry-run-complete', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    // Return a single function that cleans up all subscriptions
    return () => {
        for (const unlisten of unlisteners) {
            unlisten()
        }
    }
}

/** Statistics derived from write operation progress. */
export interface WriteOperationStats {
    /** Percentage complete (0-100) based on bytes if available, otherwise files */
    percentComplete: number
    /** Bytes per second (0 if not enough data) */
    bytesPerSecond: number
    /** Estimated time remaining in seconds (null if not enough data) */
    estimatedSecondsRemaining: number | null
    /** Elapsed time in seconds */
    elapsedSeconds: number
}

/** Derives ETA, speed, and percent from a progress event. Pair with Date.now() from when the operation started. */
export function calculateOperationStats(event: WriteProgressEvent, startTime: number): WriteOperationStats {
    const now = Date.now()
    const elapsedMs = now - startTime
    const elapsedSeconds = elapsedMs / 1000

    // Calculate percent complete (prefer bytes over files for accuracy)
    let percentComplete = 0
    if (event.bytesTotal > 0) {
        percentComplete = (event.bytesDone / event.bytesTotal) * 100
    } else if (event.filesTotal > 0) {
        percentComplete = (event.filesDone / event.filesTotal) * 100
    }

    // Calculate speed (bytes per second)
    const bytesPerSecond = elapsedSeconds > 0 ? event.bytesDone / elapsedSeconds : 0

    // Calculate ETA
    let estimatedSecondsRemaining: number | null = null
    if (bytesPerSecond > 0 && event.bytesTotal > 0) {
        const bytesRemaining = event.bytesTotal - event.bytesDone
        estimatedSecondsRemaining = bytesRemaining / bytesPerSecond
    } else if (elapsedSeconds > 0 && event.filesTotal > 0 && event.filesDone > 0) {
        // Fallback to file-based ETA
        const filesPerSecond = event.filesDone / elapsedSeconds
        const filesRemaining = event.filesTotal - event.filesDone
        estimatedSecondsRemaining = filesRemaining / filesPerSecond
    }

    return {
        percentComplete: Math.min(100, Math.max(0, percentComplete)),
        bytesPerSecond,
        estimatedSecondsRemaining,
        elapsedSeconds,
    }
}

/**
 * Formats bytes as human-readable string (like "1.5 GB").
 */
export function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${String(bytes)} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

/**
 * Formats seconds as human-readable duration (like "2m 30s").
 */
export function formatDuration(seconds: number): string {
    if (seconds < 60) return `${String(Math.round(seconds))}s`
    if (seconds < 3600) {
        const mins = Math.floor(seconds / 60)
        const secs = Math.round(seconds % 60)
        return secs > 0 ? `${String(mins)}m ${String(secs)}s` : `${String(mins)}m`
    }
    const hours = Math.floor(seconds / 3600)
    const mins = Math.round((seconds % 3600) / 60)
    return mins > 0 ? `${String(hours)}h ${String(mins)}m` : `${String(hours)}h`
}
