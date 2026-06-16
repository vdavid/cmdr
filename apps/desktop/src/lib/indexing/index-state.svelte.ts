/**
 * Reactive state for drive index scanning status.
 * Tracks whether a scan is running and provides progress info.
 */

import { commands } from '$lib/ipc/bindings'
import {
  onIndexAggregationComplete,
  onIndexAggregationProgress,
  onIndexReplayComplete,
  onIndexReplayProgress,
  onIndexRescanNotification,
  onIndexScanComplete,
  onIndexScanProgress,
  onIndexScanStarted,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

// Scan state
let scanning = $state(false)
let entriesScanned = $state(0)
let dirsFound = $state(0)
let bytesScanned = $state(0)
let scanStartedAt = $state(0)

// Per-scan calibration, set once at scan start (or backfilled on late-join). These pick the
// progress tier and seed the tier-1 ETA. `priorTotalEntries`/`priorScanDurationMs` come from
// the previous completed scan; `volumeUsedBytes` is the scanned volume's used bytes at start.
let priorTotalEntries = $state<number | null>(null)
let priorScanDurationMs = $state<number | null>(null)
let volumeUsedBytes = $state<number | null>(null)

// Replay state
let replaying = $state(false)
let replayEventsProcessed = $state(0)
let replayEstimatedTotal = $state(0)
let replayStartedAt = $state(0)

// Monotonic counter bumped by every scan/replay event. Prevents the `get_index_status`
// IPC response (which can arrive late) from overwriting state that an event already set.
let eventVersion = 0

// Aggregation state
let aggregating = $state(false)
let aggregationPhase = $state('')
let aggregationCurrent = $state(0)
let aggregationTotal = $state(0)
let aggregationStartedAt = $state(0)

// Reactive getters
export function isScanning(): boolean {
  return scanning
}

export function getEntriesScanned(): number {
  return entriesScanned
}

export function getDirsFound(): number {
  return dirsFound
}

export function getBytesScanned(): number {
  return bytesScanned
}

export function getScanStartedAt(): number {
  return scanStartedAt
}

export function getPriorTotalEntries(): number | null {
  return priorTotalEntries
}

export function getPriorScanDurationMs(): number | null {
  return priorScanDurationMs
}

export function getVolumeUsedBytes(): number | null {
  return volumeUsedBytes
}

export function isAggregating(): boolean {
  return aggregating
}

export function getAggregationPhase(): string {
  return aggregationPhase
}

export function getAggregationCurrent(): number {
  return aggregationCurrent
}

export function getAggregationTotal(): number {
  return aggregationTotal
}

export function getAggregationStartedAt(): number {
  return aggregationStartedAt
}

export function isReplaying(): boolean {
  return replaying
}

export function getReplayEventsProcessed(): number {
  return replayEventsProcessed
}

export function getReplayEstimatedTotal(): number {
  return replayEstimatedTotal
}

export function getReplayStartedAt(): number {
  return replayStartedAt
}

/** Reset scan counters (called on new scan start). */
function resetCounters() {
  entriesScanned = 0
  dirsFound = 0
  bytesScanned = 0
}

function resetAggregation() {
  aggregating = false
  aggregationPhase = ''
  aggregationCurrent = 0
  aggregationTotal = 0
  aggregationStartedAt = 0
}

function resetReplay() {
  replaying = false
  replayEventsProcessed = 0
  replayEstimatedTotal = 0
  replayStartedAt = 0
}

// Maps the backend's typed rescan-reason discriminator to its catalog message
// key (resolved via `tString` at toast time). Branching on the typed `reason`
// enum, not on message wording — copy lives in `messages/en/indexing.json`.
const rescanReasonToMessageKey: Record<string, MessageKey> = {
  stale_index: 'indexing.rescan.staleIndex',
  journal_gap: 'indexing.rescan.journalGap',
  replay_overflow: 'indexing.rescan.replayOverflow',
  too_many_subdir_rescans: 'indexing.rescan.tooManySubdirRescans',
  watcher_start_failed: 'indexing.rescan.watcherStartFailed',
  reconciler_buffer_overflow: 'indexing.rescan.reconcilerBufferOverflow',
  incomplete_previous_scan: 'indexing.rescan.incompletePreviousScan',
  watcher_channel_overflow: 'indexing.rescan.watcherChannelOverflow',
}

// Event listener cleanup handles
const unlistenHandles: UnlistenFn[] = []

/** Set up listeners for index scan events. Call once during app init. */
export async function initIndexState(): Promise<void> {
  const unlistenStarted = await onIndexScanStarted((payload) => {
    eventVersion++
    scanning = true
    resetCounters()
    resetAggregation()
    resetReplay()
    scanStartedAt = Date.now()
    priorTotalEntries = payload.priorTotalEntries
    priorScanDurationMs = payload.priorScanDurationMs
    volumeUsedBytes = payload.volumeUsedBytes
  })
  unlistenHandles.push(unlistenStarted)

  const unlistenProgress = await onIndexScanProgress((payload) => {
    entriesScanned = payload.entriesScanned
    dirsFound = payload.dirsFound
    bytesScanned = payload.bytesScanned
  })
  unlistenHandles.push(unlistenProgress)

  const unlistenComplete = await onIndexScanComplete((payload) => {
    eventVersion++
    scanning = false
    entriesScanned = payload.totalEntries
    dirsFound = payload.totalDirs
  })
  unlistenHandles.push(unlistenComplete)

  const unlistenAggregation = await onIndexAggregationProgress((payload) => {
    const { phase, current, total } = payload
    if (!aggregating || phase !== aggregationPhase) {
      aggregationStartedAt = Date.now()
    }
    if (!aggregating) {
      aggregating = true
    }
    aggregationPhase = phase
    aggregationCurrent = current
    aggregationTotal = total
  })
  unlistenHandles.push(unlistenAggregation)

  const unlistenAggComplete = await onIndexAggregationComplete(() => {
    resetAggregation()
  })
  unlistenHandles.push(unlistenAggComplete)

  const unlistenRescan = await onIndexRescanNotification((payload) => {
    const messageKey = rescanReasonToMessageKey[payload.reason] ?? 'indexing.rescan.fallback'
    addToast(tString(messageKey), { level: 'info', timeoutMs: 8000, id: 'index-rescan' })
  })
  unlistenHandles.push(unlistenRescan)

  const unlistenReplayProgress = await onIndexReplayProgress((payload) => {
    if (!replaying) {
      replaying = true
      scanning = false
      replayStartedAt = Date.now()
    }
    replayEventsProcessed = payload.eventsProcessed
    replayEstimatedTotal = payload.estimatedTotal ?? 0
  })
  unlistenHandles.push(unlistenReplayProgress)

  const unlistenReplayComplete = await onIndexReplayComplete(() => {
    eventVersion++
    resetReplay()
    scanning = false
  })
  unlistenHandles.push(unlistenReplayComplete)

  // Query current status to catch scans already in progress before the frontend loaded.
  // The scan starts in Tauri's setup() hook, so the 'index-scan-started' event may fire
  // before the frontend's event listeners are registered.
  //
  // Guard: snapshot eventVersion before the IPC call. If any scan/replay event arrived
  // while the response was in flight, the event's state is more recent, so skip the IPC result.
  const versionBeforeIpc = eventVersion
  try {
    const res = await commands.getIndexStatus()
    if (res.status === 'ok' && res.data.scanning && eventVersion === versionBeforeIpc) {
      scanning = true
      entriesScanned = res.data.entriesScanned
      dirsFound = res.data.dirsFound
      bytesScanned = res.data.bytesScanned
      // The tier-2 denominator rides the top-level response (stashed calibration).
      volumeUsedBytes = res.data.volumeUsedBytes
      // The tier-1 calibration lives in the nested meta-backed `indexStatus`. Its values are
      // the PREVIOUS completed scan's totals (the completion handler is the only writer), which
      // is exactly the tier-1 denominator — not the live counters above. Meta values are TEXT,
      // so `Number()`-parse and guard NaN → null.
      const indexStatus = res.data.indexStatus
      priorTotalEntries = parseMetaNumber(indexStatus?.totalEntries)
      priorScanDurationMs = parseMetaNumber(indexStatus?.scanDurationMs)
      // No scan-start wall-clock on late-join: the percent still works (elapsed-free), but the
      // tier-1 ETA seed and elapsed extrapolation have nothing until the sliding window fills.
      // Accepted graceful degradation.
    }
  } catch {
    // Indexing not initialized or unavailable: no-op
  }
}

/** Parse a TEXT meta value (e.g. `IndexStatus.totalEntries`) to a number, or `null` when
 *  absent or unparseable. */
function parseMetaNumber(value: string | null | undefined): number | null {
  if (value == null) return null
  const parsed = Number(value)
  return Number.isNaN(parsed) ? null : parsed
}

/** Clean up all listeners. Call during app teardown. */
export function destroyIndexState(): void {
  for (const unlisten of unlistenHandles) {
    unlisten()
  }
  unlistenHandles.length = 0
}
