/**
 * Reactive store for the set of "busy" volumes.
 *
 * A volume is busy while a copy / move / delete operation reads from or writes
 * to it. The backend computes the set (union over active write ops) and pushes
 * it via a single `volumes-busy-changed` event whenever membership changes.
 * The volume picker reads this to disable Eject for a device mid-transfer, so a
 * disconnect can't truncate an in-flight file.
 *
 * Call `initVolumeBusyStore()` once at app startup (before components mount).
 */

import { SvelteSet } from 'svelte/reactivity'
import { type UnlistenFn } from '@tauri-apps/api/event'
import { getBusyVolumeIds, onVolumesBusyChanged } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const logger = getAppLogger('volume-busy-store')

// Stable reactive container: mutate in place (clear + add), never reassign.
const busyVolumeIds = new SvelteSet<string>()
let receivedEvent = false
let initialized = false
let unlistenBusyChanged: UnlistenFn | undefined

/** Replaces the busy set with `ids`, mutating the SvelteSet so readers re-run. */
function setBusy(ids: string[]): void {
  busyVolumeIds.clear()
  for (const id of ids) busyVolumeIds.add(id)
}

/** Returns whether the given volume currently has an operation in progress. Reactive. */
export function isVolumeBusy(volumeId: string): boolean {
  return busyVolumeIds.has(volumeId)
}

/**
 * Initializes the volume-busy store.
 *
 * 1. Subscribes to `volumes-busy-changed` events from the backend.
 * 2. Bootstraps the initial set via IPC (in case the backend event fired before
 *    we subscribed, or no op is running yet).
 *
 * Idempotent: calling multiple times is safe.
 */
export async function initVolumeBusyStore(): Promise<void> {
  if (initialized) return

  unlistenBusyChanged = await onVolumesBusyChanged((payload) => {
    receivedEvent = true
    setBusy(payload.volumeIds)
    logger.debug('volumes-busy-changed: {count} busy', { count: payload.volumeIds.length })
  })

  // Bootstrap: fetch the current set (the event may have fired before we
  // subscribed, or nothing is running yet). Only apply if no event arrived.
  const ids = await getBusyVolumeIds()
  if (!receivedEvent) {
    setBusy(ids)
  }

  initialized = true
  logger.debug('Volume-busy store initialized')
}

/** Cleans up the volume-busy store. Call on app shutdown. */
export function cleanupVolumeBusyStore(): void {
  unlistenBusyChanged?.()
  unlistenBusyChanged = undefined
  busyVolumeIds.clear()
  receivedEvent = false
  initialized = false
}
