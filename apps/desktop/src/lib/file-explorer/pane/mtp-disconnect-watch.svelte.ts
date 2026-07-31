/**
 * Watches for the MTP device a pane is browsing being unplugged, and reports it
 * so the pane can fall back to its previous volume.
 *
 * The listener is registered inside an `$effect` that READS `volumeId`, so
 * Svelte re-registers it whenever the pane switches volume. That's what keeps
 * the callback from closing over a stale device id and either missing a
 * disconnect or firing on the wrong one.
 */

import { onMtpDeviceDisconnected } from '$lib/tauri-commands'
import { isMtpVolumeId } from '$lib/mtp'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

export interface MtpDisconnectWatchDeps {
  getVolumeId: () => string
  /** The pane's MTP device went away; fall back to the previous volume. */
  onFatal: (message: string) => void
}

/**
 * The device id inside a storage-specific MTP volume id (`mtp-2097152:65537` →
 * `mtp-2097152`), or `null` when the id isn't MTP or carries no storage segment.
 *
 * Split on the LAST colon: the storage id is the trailing numeric segment, and a
 * serial-based device id can itself contain a colon. Mirrors the Rust
 * `cmdr_fs::volume::mtp_ids::device_id_of_volume`.
 */
export function mtpDeviceIdOfVolume(volumeId: string): string | null {
  if (!isMtpVolumeId(volumeId) || !volumeId.includes(':')) return null
  return volumeId.slice(0, volumeId.lastIndexOf(':'))
}

export function createMtpDisconnectWatch(deps: MtpDisconnectWatchDeps): void {
  $effect(() => {
    // Reading the volume id here is what makes Svelte track it, so the listener
    // is torn down and re-registered on every volume switch.
    const deviceId = mtpDeviceIdOfVolume(deps.getVolumeId())
    if (!deviceId) return

    const listenerPromise = onMtpDeviceDisconnected((event) => {
      if (event.deviceId !== deviceId) return
      log.warn('MTP device disconnected while viewing: {deviceId}, triggering fallback', {
        deviceId: event.deviceId,
      })
      deps.onFatal('Device disconnected')
    })

    return () => {
      void listenerPromise
        .then((unsub) => {
          unsub()
        })
        .catch(() => {})
    }
  })
}
