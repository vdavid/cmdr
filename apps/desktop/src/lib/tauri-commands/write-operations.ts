// Copy/move/delete operations + event handlers

import { type Event, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
import type {
  ConflictResolution,
  ScanPreviewStartResult,
  ScanPreviewTotals,
  SortColumn,
  SortOrder,
  WriteOperationConfig,
  WriteOperationError,
  WriteOperationStartResult,
} from '../file-explorer/types'
// Event payload types now come from the generated typed-events bindings, so the
// Rust struct shapes (write-operations sink + scan-preview) drive the FE types.
import type {
  CompressedSizeEstimate,
  ConflictInfo,
  DryRunResult,
  Initiator,
  OperationStatus,
  OperationSummary,
  ScanPreviewCancelledEvent,
  ScanPreviewCompleteEvent,
  ScanPreviewErrorEvent,
  ScanPreviewProgressEvent,
  ScanProgressEvent,
  WriteCancelledEvent,
  WriteCompleteEvent,
  WriteConflictEvent,
  WriteErrorEvent,
  WriteProgressEvent,
  WriteSettledEvent,
  WriteSourceItemDoneEvent,
} from '$lib/ipc/bindings'

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
  WriteSettledEvent,
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
  ScanPreviewTotals,
  CompressedSizeEstimate,
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
  // Compress-mode scans pass `true` so the local walk samples a compressed-size
  // estimate. Ignored for remote sources (never sampled).
  sampleForEstimate?: boolean,
): Promise<ScanPreviewStartResult> {
  return commands.startScanPreview(
    sources,
    sourceVolumeId ?? null,
    sortColumn,
    sortOrder,
    progressIntervalMs ?? null,
    sampleForEstimate ?? null,
  )
}

export async function cancelScanPreview(previewId: string): Promise<void> {
  await commands.cancelScanPreview(previewId)
}

/** Returns the cached scan-preview totals if the scan has completed; `null` otherwise.
 * Used to recover FE display state when scan events fire before listeners attach
 * (M2a's watcher-backed oracle can finish in ~5 ms, beating the FE round-trip). */
export async function checkScanPreviewStatus(previewId: string): Promise<ScanPreviewTotals | null> {
  return commands.checkScanPreviewStatus(previewId)
}

export async function onScanPreviewProgress(callback: (event: ScanPreviewProgressEvent) => void): Promise<UnlistenFn> {
  return events.scanPreviewProgress.listen((event) => {
    callback(event.payload)
  })
}

export async function onScanPreviewComplete(callback: (event: ScanPreviewCompleteEvent) => void): Promise<UnlistenFn> {
  return events.scanPreviewComplete.listen((event) => {
    callback(event.payload)
  })
}

export async function onScanPreviewError(callback: (event: ScanPreviewErrorEvent) => void): Promise<UnlistenFn> {
  return events.scanPreviewError.listen((event) => {
    callback(event.payload)
  })
}

export async function onScanPreviewCancelled(
  callback: (event: ScanPreviewCancelledEvent) => void,
): Promise<UnlistenFn> {
  return events.scanPreviewCancelled.listen((event) => {
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
  initiator?: Initiator,
): Promise<WriteOperationStartResult> {
  const res = await commands.copyFiles(sources, destination, config ?? null, initiator ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Uses instant rename for same-filesystem, copy+delete for cross-filesystem. Same events as copyFiles. */
export async function moveFiles(
  sources: string[],
  destination: string,
  config?: WriteOperationConfig,
  initiator?: Initiator,
): Promise<WriteOperationStartResult> {
  const res = await commands.moveFiles(sources, destination, config ?? null, initiator ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Recursively deletes files and directories. Same events as copyFiles. */
export async function deleteFiles(
  sources: string[],
  config?: WriteOperationConfig,
  volumeId?: string,
  initiator?: Initiator,
): Promise<WriteOperationStartResult> {
  const res = await commands.deleteFiles(sources, volumeId ?? null, config ?? null, initiator ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Moves files to macOS Trash. Same events as copyFiles but with operationType: trash. */
export async function trashFiles(
  sources: string[],
  itemSizes?: number[],
  config?: WriteOperationConfig,
  initiator?: Initiator,
): Promise<WriteOperationStartResult> {
  const res = await commands.trashFiles(sources, itemSizes ?? null, config ?? null, initiator ?? null)
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
  return events.writeProgress.listen((event) => {
    callback(event.payload)
  })
}

export async function onWriteComplete(callback: (event: WriteCompleteEvent) => void): Promise<UnlistenFn> {
  return events.writeComplete.listen((event) => {
    callback(event.payload)
  })
}

export async function onWriteError(callback: (event: WriteErrorEvent) => void): Promise<UnlistenFn> {
  return events.writeError.listen((event) => {
    callback(event.payload)
  })
}

export async function onWriteCancelled(callback: (event: WriteCancelledEvent) => void): Promise<UnlistenFn> {
  return events.writeCancelled.listen((event) => {
    callback(event.payload)
  })
}

/** Emitted once per op after the spawned background task has fully torn
 *  down. See `WriteSettledEvent` for the ordering contract relative to the
 *  terminal outcome event. */
export async function onWriteSettled(callback: (event: WriteSettledEvent) => void): Promise<UnlistenFn> {
  return events.writeSettled.listen((event) => {
    callback(event.payload)
  })
}

/** Only emitted in Stop conflict resolution mode. */
export async function onWriteConflict(callback: (event: WriteConflictEvent) => void): Promise<UnlistenFn> {
  return events.writeConflict.listen((event) => {
    callback(event.payload)
  })
}

/** Emitted as each top-level source item finishes, for gradual deselection. */
export async function onWriteSourceItemDone(callback: (event: WriteSourceItemDoneEvent) => void): Promise<UnlistenFn> {
  return events.writeSourceItemDone.listen((event) => {
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
