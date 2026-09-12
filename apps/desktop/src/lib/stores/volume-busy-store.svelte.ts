/**
 * Reactive stores for the two backend-pushed volume sets that gate Eject.
 *
 * - **Busy**: a copy / move / delete operation reads from or writes to the
 *   volume. The backend computes the set (union over active write ops) and
 *   pushes it via `volumes-busy-changed` whenever membership changes. The volume
 *   picker reads it to disable Eject mid-transfer, so a disconnect can't truncate
 *   an in-flight file.
 * - **Ejecting**: the volume's eject is still running. The backend pushes it via
 *   `volumes-ejecting-changed`, and the picker shows that Eject control in
 *   progress. A second request would only join the running eject anyway.
 *
 * Call `initVolumeBusyStore()` once at app startup (before components mount); it
 * starts both.
 */

import { SvelteSet } from 'svelte/reactivity'
import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  getBusyVolumeIds,
  getEjectingVolumeIds,
  onVolumesBusyChanged,
  onVolumesEjectingChanged,
} from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const logger = getAppLogger('volume-busy-store')

/** Where one set comes from: its live event and its bootstrap command. */
interface VolumeIdSetSource {
  /** The event's wire name, for the debug log. */
  eventName: string
  subscribe: (handler: (payload: { volumeIds: string[] }) => void) => Promise<UnlistenFn>
  bootstrap: () => Promise<string[]>
}

/**
 * One backend-pushed set of volume IDs. It subscribes first, then bootstraps over
 * IPC, and an event that arrives before the bootstrap resolves wins.
 */
function createVolumeIdSet(source: VolumeIdSetSource) {
  // Stable reactive container: mutate in place (clear + add), never reassign.
  const ids = new SvelteSet<string>()
  let receivedEvent = false
  let unlisten: UnlistenFn | undefined

  /** Replaces the set with `next`, mutating the SvelteSet so readers re-run. */
  function replace(next: string[]): void {
    ids.clear()
    for (const id of next) ids.add(id)
  }

  return {
    has: (volumeId: string): boolean => ids.has(volumeId),
    async init(): Promise<void> {
      unlisten = await source.subscribe((payload) => {
        receivedEvent = true
        replace(payload.volumeIds)
        logger.debug('{eventName}: {count} volume(s)', {
          eventName: source.eventName,
          count: payload.volumeIds.length,
        })
      })
      // Bootstrap: the event may have fired before we subscribed, or nothing has
      // changed yet. Only apply it if no event arrived meanwhile.
      const current = await source.bootstrap()
      if (!receivedEvent) replace(current)
    },
    cleanup(): void {
      unlisten?.()
      unlisten = undefined
      ids.clear()
      receivedEvent = false
    },
  }
}

// The commands are reached through arrows, never read at module load: a test that
// mocks `$lib/tauri-commands` with only the exports IT needs still imports this module.
const busyVolumes = createVolumeIdSet({
  eventName: 'volumes-busy-changed',
  subscribe: (handler) => onVolumesBusyChanged(handler),
  bootstrap: () => getBusyVolumeIds(),
})
const ejectingVolumes = createVolumeIdSet({
  eventName: 'volumes-ejecting-changed',
  subscribe: (handler) => onVolumesEjectingChanged(handler),
  bootstrap: () => getEjectingVolumeIds(),
})
let initialized = false

/** Returns whether the given volume currently has an operation in progress. Reactive. */
export function isVolumeBusy(volumeId: string): boolean {
  return busyVolumes.has(volumeId)
}

/** Returns whether the given volume's eject is still running. Reactive. */
export function isVolumeEjecting(volumeId: string): boolean {
  return ejectingVolumes.has(volumeId)
}

/**
 * Initializes both sets: each subscribes to its backend event, then bootstraps
 * its initial members via IPC.
 *
 * Idempotent: calling multiple times is safe.
 */
export async function initVolumeBusyStore(): Promise<void> {
  if (initialized) return
  await Promise.all([busyVolumes.init(), ejectingVolumes.init()])
  initialized = true
  logger.debug('Volume-busy store initialized')
}

/** Cleans up both sets. Call on app shutdown. */
export function cleanupVolumeBusyStore(): void {
  busyVolumes.cleanup()
  ejectingVolumes.cleanup()
  initialized = false
}
