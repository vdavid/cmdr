// Drive-indexing event listeners: typed wrappers over the generated `events.*`
// helpers. Each payload is the `tauri-specta` event type, checked at compile
// time against the matching Rust struct. Call the returned `UnlistenFn` in
// `onDestroy` (or app teardown) to avoid leaks.

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  events,
  type AggregationProgressEvent,
  type IndexDirUpdatedEvent,
  type IndexFreshnessChangedEvent,
  type IndexMemoryWarningEvent,
  type IndexReplayCompleteEvent,
  type IndexReplayProgressEvent,
  type IndexRescanNotificationEvent,
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

/** Fires during aggregation with the current phase and progress. */
export function onIndexAggregationProgress(callback: (payload: AggregationProgressEvent) => void): Promise<UnlistenFn> {
  return events.indexAggregationProgress.listen((event) => {
    callback(event.payload)
  })
}

/** Fires when aggregation finishes. Payloadless. */
export function onIndexAggregationComplete(callback: () => void): Promise<UnlistenFn> {
  return events.indexAggregationComplete.listen(() => {
    callback()
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
export function onIndexFreshnessChanged(
  callback: (payload: IndexFreshnessChangedEvent) => void,
): Promise<UnlistenFn> {
  return events.indexFreshnessChanged.listen((event) => {
    callback(event.payload)
  })
}
