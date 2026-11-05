/**
 * Which drives have their index paused for disk space, so every folder size on them
 * carries a "may be out of date" warning.
 *
 * While the disk the index lives on is nearly full, the backend pauses indexing and
 * keeps serving the last-known sizes (`lifecycle/DETAILS.md` § "Pausing for disk
 * space"). Those sizes stop following the disk, so the size column says so. The
 * badge reads the same fact off its own status fetch (`drive-index-status.ts`); this
 * map is for the per-row readers (the lists, the status bar), which have a volume id
 * and no status.
 *
 * Fed by `index-freshness-changed`: `paused` adds the volume, any other freshness
 * drops it. A window that opens mid-pause missed that event, so the first read of a
 * volume asks the backend once (`getSpacePause` backfills lazily).
 */

import { SvelteMap } from 'svelte/reactivity'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getVolumeIndexStatusById, onIndexFreshnessChanged } from '#lib/tauri-commands/indexing.js'
import type { VolumeIndexStatus } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

/** A paused drive: whether it's the drive holding the index, which picks the copy. */
export interface SpacePause {
  holdsIndexStorage: boolean
}

const paused = new SvelteMap<string, SpacePause>()
/** Volumes whose status was already asked for, so a read backfills once, not per render. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- deliberately non-reactive: `getSpacePause` adds to it during a reactive read, which a `SvelteSet` would reject as an unsafe state mutation, and nothing renders from it.
const asked = new Set<string>()
let unlisten: UnlistenFn | null = null
/** Bumped by every init and destroy, so a registration that lands after a teardown stops itself. */
let generation = 0

/** The pause a status describes, or `undefined` when it isn't paused. Pure. */
export function spacePauseFromStatus(status: Pick<VolumeIndexStatus, 'freshness' | 'holdsIndexStorage'>) {
  return status.freshness === 'paused' ? { holdsIndexStorage: status.holdsIndexStorage } : undefined
}

async function refresh(volumeId: string): Promise<void> {
  try {
    const res = await getVolumeIndexStatusById(volumeId)
    if (res.status !== 'ok') return
    const pause = spacePauseFromStatus(res.data)
    if (pause) paused.set(volumeId, pause)
    else paused.delete(volumeId)
  } catch {
    // No backend (a test harness) or the volume vanished: no warning, which is the safe default.
  }
}

/** Listen for freshness changes. Call once per window, before anything reads. */
export async function initSpacePause(): Promise<void> {
  const myGeneration = ++generation
  const registered = await onIndexFreshnessChanged((payload) => {
    asked.add(payload.volumeId)
    // The event carries no `holdsIndexStorage`, so a pause fetches the status for it.
    if (payload.freshness === 'paused') void refresh(payload.volumeId)
    else paused.delete(payload.volumeId)
  })
  // Torn down (or started again) while the listener was registering: this one is
  // nobody's to stop later, so stop it now.
  if (myGeneration !== generation) {
    registered()
    return
  }
  unlisten = registered
}

export function destroySpacePause(): void {
  generation++
  unlisten?.()
  unlisten = null
  paused.clear()
  asked.clear()
}

/** Whether `volumeId`'s index is paused for disk space, and on which drive. Reactive. */
export function getSpacePause(volumeId: string): SpacePause | undefined {
  if (!asked.has(volumeId)) {
    asked.add(volumeId)
    // Off the reactive read: the answer lands in the map, which re-runs the reader.
    queueMicrotask(() => void refresh(volumeId))
  }
  return paused.get(volumeId)
}

/** The tooltip for a folder size the pause may have left out of date. */
export function pausedSizeTooltipKey(pause: SpacePause): MessageKey {
  return pause.holdsIndexStorage
    ? 'fileExplorer.dirSize.pausedForSpaceThisDrive'
    : 'fileExplorer.dirSize.pausedForSpaceOtherDrive'
}
