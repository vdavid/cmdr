// Reactive per-drive index FRESHNESS for the badges in the volume switcher.
// Holds a `volumeId → VolumeIndexStatus` map (the dot color + last-scan facts
// for the menu and footer), fetches it on demand (active drive + dropdown rows),
// and keeps it fresh by SUBSCRIBING to the indexing events rather than polling
// (the badge re-renders the moment a scan starts/completes or freshness flips).
// Mirrors `volume-space-manager.svelte.ts`.
//
// Responsibility split: this manager owns FRESHNESS/menu facts only. LIVE scan
// progress (the scanning tooltip's count/elapsed/bar) comes from `index-state`
// (the single live-activity source), which the badge reads per-volume via
// `getVolumeActivity`. So there's no live-progress map here.

import { SvelteMap } from 'svelte/reactivity'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'
import {
  getVolumeIndexStatusById,
  onIndexFreshnessChanged,
  onIndexScanStarted,
  onIndexScanComplete,
} from '$lib/tauri-commands/indexing'
import type { VolumeInfo } from '../types'

/**
 * Whether a switcher entry is a real DRIVE row that can carry an index badge.
 * Excludes favorites and the synthetic `network` / `search-results` entries
 * (the plan: badges only on real drives, not Favorites/groups), plus mounted
 * disk images (transient `.dmg` mounts we deliberately never index). Every
 * remaining category is a real volume the backend can report on (gray if not
 * indexed).
 *
 * This predicate is the single chokepoint for index affordances: dropping a
 * disk image here suppresses the index badge (both the active-volume spot and
 * each dropdown row), the first-connect "index this drive?" prompt, and the
 * per-volume index-status fetch, all at once.
 */
export function isDriveRow(volume: VolumeInfo): boolean {
  if (volume.category === 'favorite') return false
  if (volume.id === 'network' || volume.id === 'search-results') return false
  if (volume.isDiskImage) return false
  return true
}

export interface DriveIndexManager {
  /** Reactive map of the latest known freshness status per volume id. */
  statusMap: SvelteMap<string, VolumeIndexStatus>
  /** Fetch (or refresh) one drive's status by id. */
  fetchStatus: (volumeId: string) => Promise<void>
  /** Fetch statuses for a batch of drive rows (dropdown open). */
  fetchStatuses: (volumes: VolumeInfo[]) => Promise<void>
  destroy: () => void
}

export function createDriveIndexManager(): DriveIndexManager {
  const statusMap = new SvelteMap<string, VolumeIndexStatus>()

  const unlistens: UnlistenFn[] = []

  async function fetchStatus(volumeId: string): Promise<void> {
    // Swallow failures: a badge-status fetch can fail (IPC down, command not
    // available in a test harness, volume vanished mid-call). A failed fetch
    // degrades to "no badge for this drive", never an unhandled rejection.
    try {
      const res = await getVolumeIndexStatusById(volumeId)
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
  // Each event names its volume; refetch that one drive's status (cheap, and
  // keeps the last-scan facts in sync, which the event payload alone doesn't
  // carry). Scan start/complete flip the dot color and refresh the footer facts;
  // a freshness change is the general signal (incl. stop/cancel/disconnect).
  subscribe(onIndexFreshnessChanged((payload) => void fetchStatus(payload.volumeId)))
  subscribe(onIndexScanStarted((payload) => void fetchStatus(payload.volumeId)))
  subscribe(onIndexScanComplete((payload) => void fetchStatus(payload.volumeId)))

  function destroy() {
    for (const u of unlistens) u()
  }

  return { statusMap, fetchStatus, fetchStatuses, destroy }
}
