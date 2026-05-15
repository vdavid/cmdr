// Copy/move/delete operations + event handlers

import { type Event, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
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
  WriteSourceItemDoneEvent,
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
  WriteSourceItemDoneEvent,
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

/** Starts scanning source files immediately, emitting progress events for the Copy dialog.
 * When sourceVolumeId is provided and is not "root", the backend uses the Volume trait
 * (enabling MTP and other non-local volumes). */
export async function startScanPreview(
  sources: string[],
  sortColumn: SortColumn,
  sortOrder: SortOrder,
  progressIntervalMs?: number,
  sourceVolumeId?: string,
): Promise<ScanPreviewStartResult> {
  return commands.startScanPreview(sources, sourceVolumeId ?? null, sortColumn, sortOrder, progressIntervalMs ?? null)
}

export async function cancelScanPreview(previewId: string): Promise<void> {
  await commands.cancelScanPreview(previewId)
}

/** Checks whether scan preview results are cached (scan completed successfully). */
export async function checkScanPreviewStatus(previewId: string): Promise<boolean> {
  return commands.checkScanPreviewStatus(previewId)
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
  const res = await commands.copyFiles(sources, destination, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Uses instant rename for same-filesystem, copy+delete for cross-filesystem. Same events as copyFiles. */
export async function moveFiles(
  sources: string[],
  destination: string,
  config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
  const res = await commands.moveFiles(sources, destination, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Recursively deletes files and directories. Same events as copyFiles. */
export async function deleteFiles(
  sources: string[],
  config?: WriteOperationConfig,
  volumeId?: string,
): Promise<WriteOperationStartResult> {
  const res = await commands.deleteFiles(sources, volumeId ?? null, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Moves files to macOS Trash. Same events as copyFiles but with operationType: trash. */
export async function trashFiles(
  sources: string[],
  itemSizes?: number[],
  config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
  const res = await commands.trashFiles(sources, itemSizes ?? null, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function cancelWriteOperation(operationId: string, rollback: boolean): Promise<void> {
  await commands.cancelWriteOperation(operationId, rollback)
}

export async function cancelAllWriteOperations(): Promise<void> {
  await commands.cancelAllWriteOperations()
}

/** In Stop mode, the operation pauses on conflict and waits for this call to proceed. */
export async function resolveWriteConflict(
  operationId: string,
  resolution: ConflictResolution,
  applyToAll: boolean,
): Promise<void> {
  await commands.resolveWriteConflict(operationId, resolution, applyToAll)
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
 * Formats a files-per-second rate for the progress dialog.
 *
 * - `< 3`: 1 decimal (`"0.4 files/s"`, `"1.8 files/s"`). Small values aren't useful as integers.
 * - Rounds to exactly `1`: `"1 file/s"` (singular).
 * - `>= 3`: integer (`"27 files/s"`). Decimal precision adds nothing at high rates.
 *
 * Returns `null` for rates that round to `0.0` so the caller can hide the readout
 * entirely. The previous "0 files/s" display masked the real (sub-1) rates that
 * heterogeneous-size copies produce.
 */
export function formatFilesPerSecond(rate: number): string | null {
  if (rate < 3) {
    const oneDecimal = Math.round(rate * 10) / 10
    if (oneDecimal === 0) return null
    if (oneDecimal === 1) return '1 file/s'
    return `${oneDecimal.toFixed(1)} files/s`
  }
  return `${String(Math.round(rate))} files/s`
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
