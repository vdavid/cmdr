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
import { listVolumes, refreshVolumes, onVolumesChanged, onVolumeConnectionChanged } from '$lib/tauri-commands'
import type { VolumeConnection } from '$lib/ipc/bindings'
import type { ConnectionState, VolumeInfo } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import { getSetting, setSetting } from '$lib/settings'
import { addToast } from '$lib/ui/toast'
import { shouldShowPinHint } from '$lib/file-explorer/navigation/should-show-pin-hint'
import ServersPinHintToastContent from '$lib/file-explorer/navigation/ServersPinHintToastContent.svelte'

const logger = getAppLogger('volume-store')

let volumes = $state<VolumeInfo[]>([])
let timedOut = $state(false)
let refreshing = $state(false)
let retryFailed = $state(false)
let retryFailedTimer: ReturnType<typeof setTimeout> | null = null
let receivedEvent = false
let initialized = $state(false)
let unlistenVolumesChanged: UnlistenFn | undefined
let unlistenVolumeConnectionChanged: UnlistenFn | undefined

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
 * Drops volumes repeating an ID already seen, keeping the first.
 *
 * A volume ID is identity, and several consumers feed this list straight into a
 * keyed `{#each}` (the transfer dialog's destination picker, the tab bar's name
 * map). Svelte throws `each_key_duplicate` during flush on a repeated key, and a
 * dialog that throws mid-render leaves the pane's keyboard suppressed with
 * nothing on screen to escape from.
 *
 * The backend already publishes one location per ID (a filesystem mounted twice
 * collapses to its canonical root, `volumes/DETAILS.md` § "One volume ID
 * publishes one mount root"). This is the second line of defense, at the ONE
 * place the frontend's volume list is built, so no consumer has to repeat it.
 */
function dedupeById(list: VolumeInfo[]): VolumeInfo[] {
  // Keep-the-first by index comparison rather than a seen-set: the list is every
  // mounted volume (a handful, tens at most), so the scan costs nothing and the
  // function stays free of a mutable accumulator.
  const unique = list.filter((volume, index) => list.findIndex((other) => other.id === volume.id) === index)
  if (unique.length !== list.length) {
    logger.warn('Dropped {count} {volumesNoun} repeating a volume ID already in the list', {
      count: list.length - unique.length,
      volumesNoun: pluralize(list.length - unique.length, 'volume'),
    })
  }
  return unique
}

/**
 * Teaches the user how to shorten the switcher's Network group, once, the first
 * time it holds five pinned servers.
 *
 * ❗ Here because this is the ONE place the published list lands, so a pin made
 * from the hub, the switcher's menu, the palette, or a first connect all reach
 * it. The decision itself is `should-show-pin-hint.ts`; this counts and speaks.
 */
function notePinnedCount(list: VolumeInfo[]): void {
  const hint = shouldShowPinHint({
    pinnedCount: list.filter((volume) => volume.pinned === true).length,
    favoriteCount: list.filter((volume) => volume.category === 'favorite').length,
    seen: getSetting('behavior.serversPinHintSeen'),
  })
  if (!hint) return
  setSetting('behavior.serversPinHintSeen', true)
  addToast(ServersPinHintToastContent, {
    level: 'info',
    dismissal: 'persistent',
    id: 'servers-pin-hint',
    props: { mentionFavorites: hint.mentionFavorites },
  })
}

/**
 * Widens a `volume-connection-changed` transition into the standing
 * `connectionState` the volume picker renders.
 *
 * ❗ Total: every wire variant lands on a state. The two sign-in variants used to
 * fall to `null` (the picker kept showing whatever it had), which is wrong now
 * that a row can rest in one: a server whose backend stopped retrying has to reach
 * the dot and the pane between `volumes-changed` broadcasts, and the pane's
 * signed-out view rides on exactly this mapping.
 *
 * The unions still differ the other way: `os_mount` and `saved` are decided by the
 * backend's volume listing alone (`enrich_from_volume_registry` and the servers
 * arm), so neither ever arrives on this event.
 *
 * Exported for its own test; the subscription below is its only production caller.
 */
export function toConnectionState(state: VolumeConnection): ConnectionState {
  switch (state) {
    case 'connected':
      return 'direct'
    case 'disconnected':
      return 'disconnected'
    case 'needs_credentials':
      return 'needs_sign_in'
    case 'needs_host_key_approval':
      return 'needs_host_key_approval'
  }
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
    const published = dedupeById(payload.data)
    volumes = published
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
      count: published.length,
      volumesNoun: pluralize(published.length, 'volume'),
      timedOut: payload.timedOut,
    })

    // ❗ Last, so the list, the timeout flag, and the retry state are all
    // published before anything cosmetic runs: this reads a setting and can raise
    // a toast, and neither belongs upstream of the store's own bookkeeping.
    notePinnedCount(published)
  })

  // Subscribe to per-volume connection changes so the picker dot, the
  // `currentVolumeInfo.connectionState` field, and any pane-level UI keying
  // off this volume update the moment a session flips connected/disconnected,
  // without waiting for the next `volumes-changed` (which may not fire, as the
  // volume itself didn't appear or disappear, just its session quality).
  unlistenVolumeConnectionChanged = await onVolumeConnectionChanged((payload) => {
    const { volumeId } = payload
    const state = toConnectionState(payload.state)
    const idx = volumes.findIndex((v) => v.id === volumeId)
    if (idx < 0) return
    // Replace the entry so consumers using `$derived` over `getVolumes()` re-run.
    const next = [...volumes]
    next[idx] = { ...next[idx], connectionState: state }
    volumes = next
    logger.debug('volume-connection-changed: {volumeId} → {state}', { volumeId, state })
  })

  // Bootstrap: fetch initial list via IPC (in case the backend event
  // fired before we subscribed, or hasn't fired yet)
  const result = await listVolumes()
  // Only use bootstrap data if no event has arrived yet
  if (!receivedEvent) {
    const published = dedupeById(result.data)
    volumes = published
    timedOut = result.timedOut
    logger.debug('Bootstrap: {count} {volumesNoun}', {
      count: published.length,
      volumesNoun: pluralize(published.length, 'volume'),
    })
    notePinnedCount(published)
  }

  initialized = true
  logger.debug('Volume store initialized')
}

/** Cleans up the volume store. Call on app shutdown. */
export function cleanupVolumeStore(): void {
  unlistenVolumesChanged?.()
  unlistenVolumesChanged = undefined
  unlistenVolumeConnectionChanged?.()
  unlistenVolumeConnectionChanged = undefined
  volumes = []
  timedOut = false
  refreshing = false
  retryFailed = false
  if (retryFailedTimer) clearTimeout(retryFailedTimer)
  retryFailedTimer = null
  receivedEvent = false
  initialized = false
}
