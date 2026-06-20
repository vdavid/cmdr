// Reactive per-drive index status for the freshness badges in the volume
// switcher. Holds a `volumeId → VolumeIndexStatus` map, fetches it on demand
// (active drive + dropdown rows), and keeps it fresh by SUBSCRIBING to the
// indexing events rather than polling (the badge re-renders the moment a scan
// starts/completes or freshness flips). Mirrors `volume-space-manager.svelte.ts`.
//
// It also tracks LIVE per-volume scan progress (entries scanned + start time)
// off the 500 ms `index-scan-progress` events, so a scanning badge can show a
// live count ("Indexing… 12,345 files") rather than a static, frozen-looking
// label during a long NAS/phone scan.

import { SvelteMap } from 'svelte/reactivity'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, type VolumeIndexStatus } from '$lib/ipc/bindings'
import {
  onIndexFreshnessChanged,
  onIndexScanStarted,
  onIndexScanComplete,
  onIndexScanProgress,
} from '$lib/tauri-commands/indexing'
import type { VolumeInfo } from '../types'

/**
 * Whether a switcher entry is a real DRIVE row that can carry an index badge.
 * Excludes favorites and the synthetic `network` / `search-results` entries
 * (the plan: badges only on real drives, not Favorites/groups). Every remaining
 * category is a real volume the backend can report on (gray if not indexed).
 */
export function isDriveRow(volume: VolumeInfo): boolean {
  if (volume.category === 'favorite') return false
  if (volume.id === 'network' || volume.id === 'search-results') return false
  return true
}

/**
 * The live progress of an in-flight scan for one volume, surfaced on its badge
 * so a long NAS/phone scan reads as "yes, it's working" rather than frozen.
 * `entriesScanned` is the latest 500 ms progress tick; `scanStartedAt` is the
 * `Date.now()` of the `index-scan-started` event (for the elapsed clock).
 */
export interface DriveScanProgress {
  entriesScanned: number
  scanStartedAt: number
}

export interface DriveIndexManager {
  /** Reactive map of the latest known status per volume id. */
  statusMap: SvelteMap<string, VolumeIndexStatus>
  /**
   * Live in-flight scan progress for one volume, or `undefined` when that
   * volume isn't scanning. Reactive: reading it in a template re-renders the
   * badge on every 500 ms progress tick.
   */
  getScanProgress: (volumeId: string) => DriveScanProgress | undefined
  /** Fetch (or refresh) one drive's status by id. */
  fetchStatus: (volumeId: string) => Promise<void>
  /** Fetch statuses for a batch of drive rows (dropdown open). */
  fetchStatuses: (volumes: VolumeInfo[]) => Promise<void>
  destroy: () => void
}

export function createDriveIndexManager(): DriveIndexManager {
  const statusMap = new SvelteMap<string, VolumeIndexStatus>()
  // Live per-volume scan progress, keyed by volume id. Populated only while a
  // volume is actively scanning; cleared the moment its scan stops (complete,
  // or freshness flips away from `scanning`). Keeping it separate from
  // `statusMap` means a 500 ms progress tick re-renders only the scanning
  // badge, not every drive's status.
  const scanProgressMap = new SvelteMap<string, DriveScanProgress>()

  function getScanProgress(volumeId: string): DriveScanProgress | undefined {
    return scanProgressMap.get(volumeId)
  }

  const unlistens: UnlistenFn[] = []

  async function fetchStatus(volumeId: string): Promise<void> {
    // Swallow failures: a badge-status fetch can fail (IPC down, command not
    // available in a test harness, volume vanished mid-call). A failed fetch
    // degrades to "no badge for this drive", never an unhandled rejection.
    try {
      const res = await commands.getVolumeIndexStatusById(volumeId)
      if (res.status === 'ok') {
        statusMap.set(volumeId, res.data)
      }
    } catch {
      // Intentionally ignored; the badge simply doesn't render for this drive.
    }
  }

  async function fetchStatuses(volumes: VolumeInfo[]): Promise<void> {
    await Promise.all(volumes.filter(isDriveRow).map((v) => fetchStatus(v.id)))
  }

  // Subscribe so a badge stays live without polling. Each event names its
  // volume; refetch that one drive's status (cheap, and keeps the
  // last-scan facts in sync, which the event payload alone doesn't carry).
  // Subscription failures (no Tauri runtime in a test harness) are swallowed.
  function subscribe(register: Promise<UnlistenFn>) {
    register.then((u) => unlistens.push(u)).catch(() => {})
  }
  // Freshness changes refetch status AND retire any live scan progress when the
  // volume is no longer scanning (e.g. it flipped to `fresh` or `stale`). The
  // scan-complete event is the usual clear, but a freshness change is the
  // backstop for the cases where it doesn't arrive (stop/cancel, error).
  subscribe(
    onIndexFreshnessChanged((payload) => {
      if (payload.freshness !== 'scanning') scanProgressMap.delete(payload.volumeId)
      void fetchStatus(payload.volumeId)
    }),
  )
  subscribe(
    onIndexScanStarted((payload) => {
      scanProgressMap.set(payload.volumeId, { entriesScanned: 0, scanStartedAt: Date.now() })
      void fetchStatus(payload.volumeId)
    }),
  )
  subscribe(
    onIndexScanProgress((payload) => {
      // A progress tick can arrive before this manager mounted (a scan already
      // running at app start), so there's no recorded start time. Seed it to
      // now: the elapsed clock then under-counts the pre-mount portion, which
      // is the same graceful degradation the corner hourglass accepts.
      const startedAt = scanProgressMap.get(payload.volumeId)?.scanStartedAt ?? Date.now()
      scanProgressMap.set(payload.volumeId, { entriesScanned: payload.entriesScanned, scanStartedAt: startedAt })
    }),
  )
  subscribe(
    onIndexScanComplete((payload) => {
      scanProgressMap.delete(payload.volumeId)
      void fetchStatus(payload.volumeId)
    }),
  )

  function destroy() {
    for (const u of unlistens) u()
  }

  return { statusMap, getScanProgress, fetchStatus, fetchStatuses, destroy }
}
