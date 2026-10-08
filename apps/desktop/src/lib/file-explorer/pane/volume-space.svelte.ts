/**
 * Live per-pane disk space: the reactive `volumeSpace` readout, the fetch, the
 * backend live-update listener, and the watch registration keyed by pane id.
 *
 * ❗ **Keyed by the volume the pane is ON** (`getSpaceVolume`, the pane's
 * `paneVolumeOf` answer), and it follows that volume by ANY route: a switcher
 * pick, a walk-up after an eject, a navigation into another drive. When it
 * changes, the readout clears at once, the watch moves, and a fresh answer is
 * fetched; an answer for a volume the pane has since left is dropped. Driving it
 * from the volume switch alone left a pane that an eject walked to the boot disk
 * showing the share's "232 GB of 328 GB free" on every folder after (QA round 5,
 * R4-1). A figure for the wrong volume is worse than none.
 *
 * Disk images (`.dmg`) and views with no volume (the servers hub, search results)
 * show nothing and watch nothing. Two panes on the same volume register
 * independently (keyed by pane id), so one navigating away doesn't unwatch the
 * other.
 */

import {
  getVolumeSpace,
  watchVolumeSpace,
  unwatchVolumeSpace,
  onVolumeSpaceChanged,
  type SpaceInfo,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { untrack } from 'svelte'

/** The volume a pane's space is read for: what `paneVolumeOf` answered. */
export interface SpaceVolume {
  id: string
  path: string
  /** Disk images report no meaningful space. */
  isDiskImage: boolean
  /**
   * Whether the volume answers right now: a local one always, a session-backed one
   * while its session is live. ❗ Part of the key: a saved share and the live share it
   * becomes are one id at one path, and only this says the space is there to read.
   */
  isLive: boolean
}

export interface VolumeSpaceDeps {
  paneId: 'left' | 'right'
  /** The volume the pane is on (reactive read), or `null` for a view with none. */
  getSpaceVolume: () => SpaceVolume | null
}

export interface VolumeSpace {
  /** Live space for the volume the pane is on, or null (disk image / not yet fetched / no volume). */
  readonly volumeSpace: SpaceInfo | null
  /** Fetch space again for the volume the pane is on (after an operation changed it). */
  refresh: () => Promise<void>
  /** Register for live backend disk-space events. Call once from `onMount`. */
  startListening: () => void
  /** Drop the live-event listener and the watch. Call from `onDestroy`. */
  cleanup: () => void
}

export function createVolumeSpace(deps: VolumeSpaceDeps): VolumeSpace {
  let volumeSpace = $state<SpaceInfo | null>(null)
  let unlistenSpaceChanged: UnlistenFn | undefined
  /** The volume the readout is for. Plain: the effect below is what moves it. */
  let current: SpaceVolume | null = null
  /** Bumped on every volume change, so an answer for a volume the pane left is dropped. */
  let generation = 0

  async function refresh(): Promise<void> {
    const volume = current
    const asked = generation
    if (!volume || volume.isDiskImage || !volume.isLive) {
      volumeSpace = null
      return
    }
    // ❗ Asked by the VOLUME's own path, ❌ never the pane's: a volume's space is its
    // mount's, and the pane's path can be dead (an ejected share's mount point, the
    // moment the pane resolves to the boot disk and before it walks off) or not a
    // filesystem path at all (inside an archive). One fetch for a dead path left the
    // readout blank on every folder after.
    const answer = (await getVolumeSpace(volume.path)).data
    // ❗ `null` is "this route can't tell", ❌ never "no space": a remote volume's path
    // (`sftp://`, `webdav://`) isn't in the mount table, so its figure comes only from the
    // poller, and a slow `null` here must not blank one that already arrived. A volume
    // change clears the readout in the effect below, so a stale figure can't survive this.
    if (asked === generation && answer !== null) volumeSpace = answer
  }

  $effect(() => {
    const volume = deps.getSpaceVolume()
    const same =
      volume?.id === current?.id &&
      volume?.path === current?.path &&
      volume?.isDiskImage === current?.isDiskImage &&
      volume?.isLive === current?.isLive
    if (same) return
    current = volume
    generation++
    volumeSpace = null
    untrack(() => {
      void unwatchVolumeSpace(deps.paneId)
      if (!volume || volume.isDiskImage || !volume.isLive) return
      void refresh()
      void watchVolumeSpace(deps.paneId, volume.id, volume.path)
    })
  })

  function startListening(): void {
    // Live disk-space updates from the backend poller (typed event), for the
    // volume the pane is on and no other.
    void onVolumeSpaceChanged((payload) => {
      if (current && payload.volumeId === current.id && !current.isDiskImage && current.isLive) {
        volumeSpace = payload.space
      }
    }).then((fn) => {
      unlistenSpaceChanged = fn
    })
  }

  return {
    get volumeSpace() {
      return volumeSpace
    },
    refresh,
    startListening,
    cleanup: () => {
      unlistenSpaceChanged?.()
      void unwatchVolumeSpace(deps.paneId)
    },
  }
}
