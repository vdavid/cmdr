// Drive-indexing event listeners: typed wrappers over the generated `events.*`
// helpers. Each payload is the `tauri-specta` event type, checked at compile
// time against the matching Rust struct. Call the returned `UnlistenFn` in
// `onDestroy` (or app teardown) to avoid leaks.

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type AggregationProgressEvent,
  type IndexAggregationCompleteEvent,
  type IndexDirUpdatedEvent,
  type IndexFreshnessChangedEvent,
  type IndexMemoryWarningEvent,
  type IndexPhaseChangedEvent,
  type IndexReplayCompleteEvent,
  type IndexReplayProgressEvent,
  type IndexRescanNotificationEvent,
  type IndexScanAbortedEvent,
  type IndexScanCompleteEvent,
  type IndexScanProgressEvent,
  type IndexScanStartedEvent,
} from '$lib/ipc/bindings'

/** Fires when a full scan starts, carrying the per-scan calibration. */
export function onIndexScanStarted(callback: (payload: IndexScanStartedEvent) => void): Promise<UnlistenFn> {
  return events.indexScanStarted.listen((event) => {
    callback(event.payload)
  })
}

/** Fires every ~500 ms during a full scan with the live entry/dir/byte counters. */
export function onIndexScanProgress(callback: (payload: IndexScanProgressEvent) => void): Promise<UnlistenFn> {
  return events.indexScanProgress.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when a full scan completes, carrying the final totals. */
export function onIndexScanComplete(callback: (payload: IndexScanCompleteEvent) => void): Promise<UnlistenFn> {
  return events.indexScanComplete.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Fires when a network (SMB/MTP) scan ends WITHOUT completing (disconnected,
 * canceled, timed out). Carries no completion facts — it just tells the FE to
 * clear the volume's live activity so an aborted scan leaves no stuck row.
 */
export function onIndexScanAborted(callback: (payload: IndexScanAbortedEvent) => void): Promise<UnlistenFn> {
  return events.indexScanAborted.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Fires when a volume's top-level indexing phase changes (a transition in the
 * `Scanning → Aggregating → Reconciling → Live` pipeline, plus `Replaying` and
 * `Idle`). Per-volume, unlike the global debug-window phase timeline. Drives the
 * per-volume step checklist: the FE maps the typed `phase` to a step.
 */
export function onIndexPhaseChanged(callback: (payload: IndexPhaseChangedEvent) => void): Promise<UnlistenFn> {
  return events.indexPhaseChanged.listen((event) => {
    callback(event.payload)
  })
}

/** Fires during aggregation with the current phase and progress. */
export function onIndexAggregationProgress(callback: (payload: AggregationProgressEvent) => void): Promise<UnlistenFn> {
  return events.indexAggregationProgress.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when aggregation finishes, carrying the volume whose pass completed. */
export function onIndexAggregationComplete(
  callback: (payload: IndexAggregationCompleteEvent) => void,
): Promise<UnlistenFn> {
  return events.indexAggregationComplete.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when a full rescan is triggered, carrying the reason for a user toast. */
export function onIndexRescanNotification(
  callback: (payload: IndexRescanNotificationEvent) => void,
): Promise<UnlistenFn> {
  return events.indexRescanNotification.listen((event) => {
    callback(event.payload)
  })
}

/** Fires every ~500 ms during FSEvents-journal replay with the live counters. */
export function onIndexReplayProgress(callback: (payload: IndexReplayProgressEvent) => void): Promise<UnlistenFn> {
  return events.indexReplayProgress.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when replay completes. */
export function onIndexReplayComplete(callback: (payload: IndexReplayCompleteEvent) => void): Promise<UnlistenFn> {
  return events.indexReplayComplete.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when the index computes or refreshes dir_stats, carrying the affected paths. */
export function onIndexDirUpdated(callback: (payload: IndexDirUpdatedEvent) => void): Promise<UnlistenFn> {
  return events.indexDirUpdated.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when the memory watchdog stops indexing to avoid a system crash. */
export function onIndexMemoryWarning(callback: (payload: IndexMemoryWarningEvent) => void): Promise<UnlistenFn> {
  return events.indexMemoryWarning.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Fires when a volume's index freshness changes to a NEW value (the badge
 * refreshes; the one-time stale dialog fires on the exact Fresh→Stale edge).
 */
export function onIndexFreshnessChanged(callback: (payload: IndexFreshnessChangedEvent) => void): Promise<UnlistenFn> {
  return events.indexFreshnessChanged.listen((event) => {
    callback(event.payload)
  })
}

// ============================================================================
// Drive-indexing commands
// ============================================================================
// Callers branch on the typed `Result`/error discriminant, so these are
// passthroughs — they don't unwrap the generated `Result` shape.

/** Reads the global drive-indexing status (running/idle, per-volume summary). */
export function getIndexStatus() {
  return commands.getIndexStatus()
}

/** Per-volume index status keyed by volume id (the per-drive badge surface). */
export function getVolumeIndexStatusById(volumeId: string) {
  return commands.getVolumeIndexStatusById(volumeId)
}

/** Turns on indexing for a specific drive. */
export function enableDriveIndex(volumeId: string) {
  return commands.enableDriveIndex(volumeId)
}

/** Turns off indexing for a specific drive, preserving its DB on disk. */
export function disableDriveIndex(volumeId: string) {
  return commands.disableDriveIndex(volumeId)
}

/** Forgets a drive's index entirely: stops it and deletes its index DB. */
export function forgetDriveIndex(volumeId: string) {
  return commands.forgetDriveIndex(volumeId)
}

/** Forces a fresh full rescan of a drive (the menu's "Rescan now"). */
export function rescanDriveIndex(volumeId: string) {
  return commands.rescanDriveIndex(volumeId)
}

/** Clears the local (`root`) drive index entirely. */
export function clearDriveIndex() {
  return commands.clearDriveIndex()
}
