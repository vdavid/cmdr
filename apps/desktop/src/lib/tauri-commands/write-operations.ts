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

export async function cancelWriteOperation(operationId: string, rollback: boolean): Promise<void> {
    await invoke('cancel_write_operation', { operationId, rollback })
}

export async function cancelAllWriteOperations(): Promise<void> {
    await invoke('cancel_all_write_operations')
}

/** In Stop mode, the operation pauses on conflict and waits for this call to proceed. */
export async function resolveWriteConflict(
    operationId: string,
    resolution: ConflictResolution,
    applyToAll: boolean,
): Promise<void> {
    await invoke('resolve_write_conflict', { operationId, resolution, applyToAll })
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
