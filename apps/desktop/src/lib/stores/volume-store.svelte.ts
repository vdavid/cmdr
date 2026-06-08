/**
 * Reactive store for the volume list.
 *
 * The backend pushes the full volume list via a single `volumes-changed` event
 * whenever anything changes (local mount/unmount, MTP connect/disconnect).
 * This store subscribes once and exposes the list reactively.
 *
 * Call `initVolumeStore()` once at app startup (before components mount).
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { listVolumes, refreshVolumes, onVolumesChanged, onSmbConnectionChanged } from '$lib/tauri-commands'
import type { SmbConnectionState, VolumeInfo } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'

const logger = getAppLogger('volume-store')

let volumes = $state<VolumeInfo[]>([])
let timedOut = $state(false)
let refreshing = $state(false)
let retryFailed = $state(false)
let retryFailedTimer: ReturnType<typeof setTimeout> | null = null
let receivedEvent = false
let initialized = $state(false)
let unlistenVolumesChanged: UnlistenFn | undefined
let unlistenSmbConnectionChanged: UnlistenFn | undefined

/** Returns the current volume list. Reactive. */
export function getVolumes(): VolumeInfo[] {
  return volumes
}

/** Returns whether the last volume listing timed out (some volumes may be missing). Reactive. */
export function getVolumesTimedOut(): boolean {
  return timedOut
}

/** Returns whether a volume refresh is in progress. Reactive. */
export function isVolumesRefreshing(): boolean {
  return refreshing
}

/** Returns whether a retry just completed but the listing is still timed out. Reactive.
 *  Auto-resets to false after 3 seconds. */
export function isVolumeRetryFailed(): boolean {
  return retryFailed
}

/**
 * Requests a fresh volume list from the backend.
 * The result arrives via the `volumes-changed` event (single source of truth).
 * Used by the retry button when the initial listing timed out.
 */
export function requestVolumeRefresh(): void {
  if (refreshing) return

  refreshing = true
  retryFailed = false
  if (retryFailedTimer) clearTimeout(retryFailedTimer)

  // Tell the backend to re-broadcast. The result arrives via the
  // `volumes-changed` event listener, which handles retryFailed.
  void refreshVolumes()
}

/**
 * Initializes the volume store.
 *
 * 1. Subscribes to `volumes-changed` events from the backend.
 * 2. Fetches the initial volume list via IPC as a bootstrap
 *    (the backend also emits an initial event, but the frontend
 *    may not be listening yet when it fires).
 *
 * Idempotent: calling multiple times is safe.
 */
export async function initVolumeStore(): Promise<void> {
  if (initialized) return

  // Subscribe to backend-pushed volume list updates
  unlistenVolumesChanged = await onVolumesChanged((payload) => {
    receivedEvent = true
    volumes = payload.data
    timedOut = payload.timedOut

    // Detect retry failure: we were refreshing and it's still timed out
    if (refreshing) {
      refreshing = false
      if (payload.timedOut) {
        retryFailed = true
        retryFailedTimer = setTimeout(() => {
          retryFailed = false
        }, 3000)
      }
    }

    logger.debug('volumes-changed: {count} {volumesNoun}, timedOut={timedOut}', {
      count: payload.data.length,
      volumesNoun: pluralize(payload.data.length, 'volume'),
      timedOut: payload.timedOut,
    })
  })

  // Subscribe to SMB connection-state changes so the picker dot, the
  // `currentVolumeInfo.smbConnectionState` field, and any pane-level UI keying
  // off this volume update the moment a session flips Direct/Disconnected,
  // without waiting for the next `volumes-changed` (which may not fire, as the
  // volume itself didn't appear or disappear, just its session quality).
  unlistenSmbConnectionChanged = await onSmbConnectionChanged((payload) => {
    const { volumeId } = payload
    // `needs_auth` is a reconnect-manager-only signal; the picker only tracks
    // Direct / Disconnected, so ignore other states here.
    if (payload.state !== 'direct' && payload.state !== 'disconnected') return
    const state: SmbConnectionState = payload.state
    const idx = volumes.findIndex((v) => v.id === volumeId)
    if (idx < 0) return
    // Replace the entry so consumers using `$derived` over `getVolumes()` re-run.
    const next = [...volumes]
    next[idx] = { ...next[idx], smbConnectionState: state }
    volumes = next
    logger.debug('smb-connection-changed: {volumeId} → {state}', { volumeId, state })
  })

  // Bootstrap: fetch initial list via IPC (in case the backend event
  // fired before we subscribed, or hasn't fired yet)
  const result = await listVolumes()
  // Only use bootstrap data if no event has arrived yet
  if (!receivedEvent) {
    volumes = result.data
    timedOut = result.timedOut
    logger.debug('Bootstrap: {count} {volumesNoun}', {
      count: result.data.length,
      volumesNoun: pluralize(result.data.length, 'volume'),
    })
  }

  initialized = true
  logger.debug('Volume store initialized')
}

/** Cleans up the volume store. Call on app shutdown. */
export function cleanupVolumeStore(): void {
  unlistenVolumesChanged?.()
  unlistenVolumesChanged = undefined
  unlistenSmbConnectionChanged?.()
  unlistenSmbConnectionChanged = undefined
  volumes = []
  timedOut = false
  refreshing = false
  retryFailed = false
  if (retryFailedTimer) clearTimeout(retryFailedTimer)
  retryFailedTimer = null
  receivedEvent = false
  initialized = false
}
