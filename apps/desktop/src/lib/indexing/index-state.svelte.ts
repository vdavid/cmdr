/**
 * Reactive state for drive index scanning status.
 *
 * Tracks indexing activity for ALL drives (local + SMB + MTP), not just root: a
 * per-volume map keyed by `volumeId`, fed by the per-volume scan and replay
 * events (each carries its `volumeId`). The corner hourglass shows whenever ANY
 * volume is active, and its tooltip lists every active drive.
 *
 * Aggregation is the one exception: `index-aggregation-progress` carries no
 * `volumeId` (a single writer-side signal). It runs right after a writer's scan
 * completes, so we attribute it to the volume whose scan most recently finished
 * (falling back to `root`, the common case). See § "Aggregation has no volumeId"
 * in DETAILS.md.
 *
 * Backward-compatible scalar getters (`isScanning`, `getEntriesScanned`, …) are
 * retained for the other consumers: `isScanning`/`isAggregating`/`isReplaying`
 * report "any volume active" (what the size-updating hourglass and search-unavailable
 * state want), while the scan-counter getters report the `root` volume (what
 * SearchDialog's index-build progress reads).
 */

import { SvelteMap } from 'svelte/reactivity'
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

/** The local volume's id (mirrors `DEFAULT_VOLUME_ID` in `tauri-commands/storage`). */
const ROOT_VOLUME_ID = 'root'

/** Which activity a volume is currently doing. Aggregation isn't a per-volume
 *  phase here (it has no `volumeId`); it's attributed via `aggregatingVolumeId`. */
export type IndexActivityPhase = 'scanning' | 'replaying'

/** Live indexing activity for one volume. Scan and replay carry their own
 *  `volumeId`, so both are attributable; aggregation is folded in by the
 *  consumer via `aggregatingVolumeId`. */
export interface VolumeIndexActivity {
  volumeId: string
  phase: IndexActivityPhase
  // Scan fields (phase === 'scanning')
  entriesScanned: number
  dirsFound: number
  bytesScanned: number
  scanStartedAt: number
  priorTotalEntries: number | null
  priorScanDurationMs: number | null
  volumeUsedBytes: number | null
  // Replay fields (phase === 'replaying')
  replayEventsProcessed: number
  replayEstimatedTotal: number
  replayStartedAt: number
}

function newScanActivity(volumeId: string): VolumeIndexActivity {
  return {
    volumeId,
    phase: 'scanning',
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    scanStartedAt: Date.now(),
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
  }
}

function newReplayActivity(volumeId: string): VolumeIndexActivity {
  return {
    volumeId,
    phase: 'replaying',
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    scanStartedAt: 0,
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: Date.now(),
  }
}

// Per-volume activity, keyed by volume id. An entry exists only while that
// volume is actively scanning or replaying; it's removed the moment that volume
// finishes. Reactive: reading it re-renders the corner hourglass and its tooltip.
const activity = new SvelteMap<string, VolumeIndexActivity>()

// Aggregation is a single, volume-less signal (the event carries no volumeId).
// We attribute it to whichever volume most recently completed a scan — the
// writer that's now aggregating. Falls back to `root` (the common case) before
// any scan-complete has been seen this session.
let aggregating = $state(false)
let aggregationPhase = $state('')
let aggregationCurrent = $state(0)
let aggregationTotal = $state(0)
let aggregationStartedAt = $state(0)
let lastCompletedScanVolumeId = $state(ROOT_VOLUME_ID)

// Monotonic counter bumped by every scan/replay event. Prevents the
// `get_index_status` IPC response (which can arrive late) from overwriting state
// an event already set, for the `root` backfill below.
let eventVersion = 0

// ── New multi-drive API ──────────────────────────────────────────────

/** Every volume currently scanning or replaying, in insertion order. Reactive. */
export function getActiveIndexVolumes(): VolumeIndexActivity[] {
  return [...activity.values()]
}

/** Whether ANY drive is scanning, replaying, or aggregating. The corner
 *  hourglass's visibility gate. Reactive. */
export function isAnyVolumeIndexing(): boolean {
  return activity.size > 0 || aggregating
}

/** The volume id aggregation is attributed to (the last scan to complete, or
 *  `root`). Reactive. */
export function getAggregatingVolumeId(): string {
  return lastCompletedScanVolumeId
}

// ── Backward-compatible scalar getters ───────────────────────────────
//
// `isScanning`/`isAggregating`/`isReplaying` report "any volume active" (the
// size-updating hourglass and search-unavailable consumers want global truth).
// The scan-counter getters report the `root` volume (SearchDialog's index-build
// progress is root-only).

function root(): VolumeIndexActivity | undefined {
  return activity.get(ROOT_VOLUME_ID)
}

export function isScanning(): boolean {
  for (const a of activity.values()) {
    if (a.phase === 'scanning') return true
  }
  return false
}

export function getEntriesScanned(): number {
  const r = root()
  return r?.phase === 'scanning' ? r.entriesScanned : 0
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

function resetAggregation() {
  aggregating = false
  aggregationPhase = ''
  aggregationCurrent = 0
  aggregationTotal = 0
  aggregationStartedAt = 0
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
    const a = newScanActivity(payload.volumeId)
    a.priorTotalEntries = payload.priorTotalEntries
    a.priorScanDurationMs = payload.priorScanDurationMs
    a.volumeUsedBytes = payload.volumeUsedBytes
    activity.set(payload.volumeId, a)
  })
  unlistenHandles.push(unlistenStarted)

  const unlistenProgress = await onIndexScanProgress((payload) => {
    // A progress tick can arrive before the started event (a scan already
    // running at app start, missed on a mid-scan reload). Seed an entry so the
    // hourglass still shows; the started fields stay at their defaults.
    const a = activity.get(payload.volumeId) ?? newScanActivity(payload.volumeId)
    a.phase = 'scanning'
    a.entriesScanned = payload.entriesScanned
    a.dirsFound = payload.dirsFound
    a.bytesScanned = payload.bytesScanned
    activity.set(payload.volumeId, a)
  })
  unlistenHandles.push(unlistenProgress)

  const unlistenComplete = await onIndexScanComplete((payload) => {
    eventVersion++
    // This volume's scan is done; aggregation (volume-less) follows on its
    // writer, so remember it for attribution.
    lastCompletedScanVolumeId = payload.volumeId
    activity.delete(payload.volumeId)
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
    const existing = activity.get(payload.volumeId)
    const a = existing?.phase === 'replaying' ? existing : newReplayActivity(payload.volumeId)
    a.phase = 'replaying'
    a.replayEventsProcessed = payload.eventsProcessed
    a.replayEstimatedTotal = payload.estimatedTotal ?? 0
    activity.set(payload.volumeId, a)
  })
  unlistenHandles.push(unlistenReplayProgress)

  const unlistenReplayComplete = await onIndexReplayComplete((payload) => {
    eventVersion++
    const a = activity.get(payload.volumeId)
    if (a?.phase === 'replaying') activity.delete(payload.volumeId)
  })
  unlistenHandles.push(unlistenReplayComplete)

  // Query current status to catch a `root` scan already in progress before the
  // frontend loaded (the scan starts in Tauri's setup() hook, so its
  // 'index-scan-started' event may fire before our listeners registered). This
  // backfill is root-only; SMB/MTP scans hydrate from their next progress tick.
  //
  // Guard: snapshot eventVersion before the IPC call. If any scan/replay event
  // arrived while the response was in flight, the event's state is more recent,
  // so skip the IPC result.
  const versionBeforeIpc = eventVersion
  try {
    const res = await commands.getIndexStatus()
    if (res.status === 'ok' && res.data.scanning && eventVersion === versionBeforeIpc) {
      const a = newScanActivity(ROOT_VOLUME_ID)
      // No scan-start wall-clock on late-join: the percent still works
      // (elapsed-free), but the tier-1 ETA seed and elapsed extrapolation have
      // nothing until the sliding window fills. Accepted graceful degradation.
      a.scanStartedAt = 0
      a.entriesScanned = res.data.entriesScanned
      a.dirsFound = res.data.dirsFound
      a.bytesScanned = res.data.bytesScanned
      // The tier-2 denominator rides the top-level response (stashed calibration).
      a.volumeUsedBytes = res.data.volumeUsedBytes
      // The tier-1 calibration lives in the nested meta-backed `indexStatus`. Its
      // values are the PREVIOUS completed scan's totals (the completion handler is
      // the only writer), which is exactly the tier-1 denominator — not the live
      // counters above. Meta values are TEXT, so `Number()`-parse and guard NaN → null.
      const indexStatus = res.data.indexStatus
      a.priorTotalEntries = parseMetaNumber(indexStatus?.totalEntries)
      a.priorScanDurationMs = parseMetaNumber(indexStatus?.scanDurationMs)
      activity.set(ROOT_VOLUME_ID, a)
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
  activity.clear()
  resetAggregation()
  lastCompletedScanVolumeId = ROOT_VOLUME_ID
}
