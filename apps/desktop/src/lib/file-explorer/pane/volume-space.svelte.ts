/**
 * Live per-pane disk space. Owns the reactive `volumeSpace` readout, the fetch,
 * the backend live-update listener, and the watch/unwatch registration keyed by
 * pane id. Lifted out of `FilePane.svelte`; the pane keeps a one-line
 * `refreshVolumeSpace` delegate (a `FilePaneAPI` export) and orchestrates the
 * watch/unwatch across mount, volume-switch, and destroy through this handle.
 *
 * Disk images (`.dmg`) report no meaningful free space, so the fetch, the live
 * event, and the volume-switch path all skip them (the pane hides the bar too).
 * Two panes on the same volume register independently (keyed by pane id), so one
 * navigating away doesn't unwatch the other.
 */

import {
  getVolumeSpace,
  watchVolumeSpace,
  unwatchVolumeSpace,
  onVolumeSpaceChanged,
  type VolumeSpaceInfo,
  type UnlistenFn,
} from '$lib/tauri-commands'

export interface VolumeSpaceDeps {
  paneId: 'left' | 'right'
  getVolumeId: () => string
  getCurrentPath: () => string
  /** The pane's disk-image flag (reactive read). Disk images report no meaningful space. */
  getIsDiskImage: () => boolean
}

export interface VolumeSpace {
  /** Live space for the pane's volume, or null (disk image / not yet fetched / virtual). */
  readonly volumeSpace: VolumeSpaceInfo | null
  /** Fetch space for the current path. A disk image clears the readout instead. */
  refresh: () => Promise<void>
  /** Register for live backend disk-space events. Call once from `onMount`. */
  startListening: () => void
  /** Start live polling for a volume + path (keyed by this pane's id). */
  watch: (volumeId: string, path: string) => void
  /** Stop live polling for this pane. */
  unwatch: () => void
  /** Clear the readout (e.g. after switching onto a disk-image volume). */
  clear: () => void
  /** Drop the live-event listener and the watch. Call from `onDestroy`. */
  cleanup: () => void
}

export function createVolumeSpace(deps: VolumeSpaceDeps): VolumeSpace {
  let volumeSpace = $state<VolumeSpaceInfo | null>(null)
  let unlistenSpaceChanged: UnlistenFn | undefined

  async function refresh(): Promise<void> {
    // Disk images report no meaningful free space; keep it null so neither the
    // bottom disk-usage bar nor the SelectionInfo free/total text renders.
    if (deps.getIsDiskImage()) {
      volumeSpace = null
      return
    }
    volumeSpace = (await getVolumeSpace(deps.getCurrentPath())).data
  }

  function startListening(): void {
    // Live disk-space updates from the backend poller (typed event). Ignore disk
    // images: no meaningful free space. We don't register a watch for them, so
    // this is a belt-and-suspenders guard against a late/stray event.
    void onVolumeSpaceChanged((payload) => {
      if (payload.volumeId === deps.getVolumeId() && !deps.getIsDiskImage()) {
        volumeSpace = {
          totalBytes: payload.totalBytes,
          availableBytes: payload.availableBytes,
        }
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
    watch: (volumeId: string, path: string) => {
      void watchVolumeSpace(deps.paneId, volumeId, path)
    },
    unwatch: () => {
      void unwatchVolumeSpace(deps.paneId)
    },
    clear: () => {
      volumeSpace = null
    },
    cleanup: () => {
      unlistenSpaceChanged?.()
      void unwatchVolumeSpace(deps.paneId)
    },
  }
}
