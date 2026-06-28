/**
 * Reactive state for drive index scanning status.
 *
 * Tracks indexing activity for ALL drives (local + SMB + MTP), not just root: a
 * per-volume map keyed by `volumeId`, fed by the per-volume scan and replay
 * events (each carries its `volumeId`). The corner hourglass shows whenever ANY
 * volume is active, and its tooltip lists every active drive.
 *
 * Aggregation is per-volume too: `index-aggregation-progress` and
 * `index-aggregation-complete` both carry the `volumeId`, so each drive's
 * aggregation is tracked in its own map entry. Two drives aggregating at once
 * each get their own progress. See § "Aggregation" in DETAILS.md.
 *
 * Backward-compatible scalar getters (`isScanning`, `getEntriesScanned`, …) are
 * retained for the other consumers: `isScanning`/`isAggregating`/`isReplaying`
 * report "any volume active" (what the size-updating hourglass and search-unavailable
 * state want), while the scan-counter getters report the `root` volume (what
 * SearchDialog's index-build progress reads).
 */

import { SvelteMap } from 'svelte/reactivity'
import { commands, type ActivityPhase } from '$lib/ipc/bindings'
import {
  onIndexAggregationComplete,
  onIndexAggregationProgress,
  onIndexPhaseChanged,
  onIndexReplayComplete,
  onIndexReplayProgress,
  onIndexRescanNotification,
  onIndexScanAborted,
  onIndexScanComplete,
  onIndexScanProgress,
  onIndexScanStarted,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/** The local volume's id (mirrors `DEFAULT_VOLUME_ID` in `tauri-commands/storage`).
 *  Exported so the checklist can tell a local scan (all four steps) from a
 *  network one (SMB/MTP: no Save/Catch-up step) without reaching into `storage`. */
export const ROOT_VOLUME_ID = 'root'

/** Which live activity a volume is currently doing. Aggregation is tracked
 *  separately (its own per-volume map), folded into the row by the consumer. */
export type IndexActivityPhase = 'scanning' | 'replaying'

/** Per-volume aggregation progress, tracked separately from scan/replay because
 *  aggregation runs on the writer thread after a scan and overlaps with nothing
 *  else for the same volume. Keyed by `volumeId` in the `aggregation` map. */
export interface AggregationActivity {
  /** One of: `saving_entries` | `loading` | `sorting` | `computing` | `writing`. */
  phase: string
  current: number
  total: number
  /** `Date.now()` when this phase began (reset on each phase change), for ETA. */
  startedAt: number
}

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

/** A zero-valued activity for a volume with no live scan/replay entry but still
 *  mid-pipeline (aggregating, or reconciling). The checklist derives its state
 *  from the volume's phase + aggregation, not from these fields, so they stay at
 *  zero and the active step (compute or catch-up) shows no scan detail. Shared by
 *  the corner indicator and the breadcrumb badge so both render those rows alike. */
export function placeholderActivity(volumeId: string): VolumeIndexActivity {
  return {
    volumeId,
    phase: 'scanning',
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    scanStartedAt: 0,
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

// Per-volume aggregation, keyed by volume id. An entry exists only while that
// volume's writer is aggregating (between its `index-aggregation-progress` and
// `index-aggregation-complete`, both volumeId-stamped). Reactive: reading it
// re-renders the corner hourglass and its tooltip.
const aggregation = new SvelteMap<string, AggregationActivity>()

// Per-volume top-level pipeline phase, keyed by volume id, fed by the
// `index-phase-changed` event (the per-volume counterpart to the global debug
// phase timeline). An entry exists only while a volume is mid-pipeline:
// `scanning` | `aggregating` | `reconciling` | `replaying`. It's removed on the
// terminal `live` / `idle` transitions and on a scan abort, so a present entry
// always means "this volume is at this step right now". This is the authoritative
// driver for the step checklist's reconcile step, which has no other event signal.
// Reactive: reading it re-renders the consumer.
const phase = new SvelteMap<string, ActivityPhase>()

// Monotonic counter bumped by every scan/replay event. Prevents the
// `get_index_status` IPC response (which can arrive late) from overwriting state
// an event already set, for the `root` backfill below.
let eventVersion = 0

// ── New multi-drive API ──────────────────────────────────────────────

/** Every volume currently scanning or replaying, in insertion order. Reactive. */
export function getActiveIndexVolumes(): VolumeIndexActivity[] {
  return [...activity.values()]
}

/** One volume's live scan/replay activity, or `undefined` when it isn't
 *  scanning or replaying. Reactive. The breadcrumb badge reads its OWN volume's
 *  activity through this to render the shared status body (the single live-activity
 *  source; the badge manager owns only freshness/menu facts, not live progress). */
export function getVolumeActivity(volumeId: string): VolumeIndexActivity | undefined {
  return activity.get(volumeId)
}

/** Whether ANY drive is scanning, replaying, or aggregating, OR is mid-pipeline
 *  in a phase with no live scan/aggregation entry (the reconcile step: scan and
 *  aggregation have both finished, only the phase event marks it). The corner
 *  hourglass's visibility gate. Including the `phase` map keeps the checklist —
 *  and its catch-up step — on screen through reconcile, instead of the surface
 *  vanishing the moment aggregation completes. Reactive. */
export function isAnyVolumeIndexing(): boolean {
  return activity.size > 0 || aggregation.size > 0 || phase.size > 0
}

/** This volume's live aggregation progress, or `undefined` when it isn't
 *  aggregating. Reactive. */
export function getVolumeAggregation(volumeId: string): AggregationActivity | undefined {
  return aggregation.get(volumeId)
}

/** This volume's current top-level pipeline phase (`scanning` / `aggregating` /
 *  `reconciling` / `replaying`), or `undefined` when it isn't mid-pipeline (idle,
 *  done, or never started). Reactive. The step checklist maps the typed
 *  phase to the active step; it's the only signal for the reconcile step. */
export function getVolumePhase(volumeId: string): ActivityPhase | undefined {
  return phase.get(volumeId)
}

/** Every volume currently aggregating, in insertion order. Reactive. Lets the
 *  indicator add a row for a volume that's aggregating with no live scan/replay
 *  entry (its scan already finished). */
export function getAggregatingVolumeIds(): string[] {
  return [...aggregation.keys()]
}

/** Every volume with a live top-level phase, in insertion order. Reactive. Lets
 *  the indicator add a row for a volume that's mid-pipeline with no live scan or
 *  aggregation entry (the reconcile step), so the checklist's catch-up step stays
 *  visible after aggregation completes. */
export function getActivePhaseVolumeIds(): string[] {
  return [...phase.keys()]
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

/** Whether ANY drive is aggregating (the size-updating hourglass and
 *  search-unavailable consumers want global truth). Reactive. */
export function isAggregating(): boolean {
  return aggregation.size > 0
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
    // This volume's scan is done; aggregation follows on its writer, attributed
    // by the volumeId-stamped aggregation events below.
    activity.delete(payload.volumeId)
  })
  unlistenHandles.push(unlistenComplete)

  const unlistenAborted = await onIndexScanAborted((payload) => {
    eventVersion++
    // A network scan ended without completing (disconnect, cancel, timeout). No
    // completion event fires for it, so clear this volume's live activity (and
    // any partial aggregation) here, or the corner indicator and badge tooltip
    // would keep a stuck "scanning" row. Freshness (the badge dot color) is
    // handled separately by the manager's freshness subscription.
    activity.delete(payload.volumeId)
    aggregation.delete(payload.volumeId)
    // The cancel/fail abort arm fires no phase event, so clear the phase here too,
    // or a stale mid-pipeline phase would linger for the aborted volume.
    phase.delete(payload.volumeId)
  })
  unlistenHandles.push(unlistenAborted)

  const unlistenPhase = await onIndexPhaseChanged((payload) => {
    // `live` / `idle` are terminal: the volume left the pipeline, so drop the
    // entry (the row is unmounting anyway). Every other phase is an active step,
    // so record it for the checklist. Branching on the typed `ActivityPhase`
    // discriminant, never message wording.
    if (payload.phase === 'live' || payload.phase === 'idle') {
      phase.delete(payload.volumeId)
    } else {
      phase.set(payload.volumeId, payload.phase)
    }
  })
  unlistenHandles.push(unlistenPhase)

  const unlistenAggregation = await onIndexAggregationProgress((payload) => {
    const { volumeId, phase, current, total } = payload
    const existing = aggregation.get(volumeId)
    // Reset the ETA clock on first sight of this volume's aggregation or on a
    // phase change, so each phase's progress extrapolates from its own start.
    const startedAt = existing && existing.phase === phase ? existing.startedAt : Date.now()
    aggregation.set(volumeId, { phase, current, total, startedAt })
  })
  unlistenHandles.push(unlistenAggregation)

  const unlistenAggComplete = await onIndexAggregationComplete((payload) => {
    aggregation.delete(payload.volumeId)
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
  aggregation.clear()
  phase.clear()
}
