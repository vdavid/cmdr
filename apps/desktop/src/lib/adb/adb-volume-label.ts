/**
 * The volume switcher's label for a device volume. The same phone can be listed
 * twice, over MTP and over ADB, under the same model name; the ADB entry gets an
 * "ADB" suffix only then, so a phone that is only reachable one way keeps its
 * plain name. Contract: `docs/specs/android-adb-backend.md` § "App wiring".
 */
import type { VolumeInfo } from '$lib/file-explorer/types'
import { tString } from '$lib/intl/messages.svelte'

/** Whether this is the ADB half of a device also listed over MTP. */
function hasMtpTwin(volume: VolumeInfo, volumes: readonly VolumeInfo[]): boolean {
  return volumes.some(
    (v) => v.id !== volume.id && v.category === 'mobile_device' && v.fsType !== 'adb' && v.name === volume.name,
  )
}

/** The switcher label: the volume's name, suffixed with "ADB" when an MTP twin shares it. */
export function deviceVolumeLabel(volume: VolumeInfo, volumes: readonly VolumeInfo[]): string {
  if (volume.fsType !== 'adb' || !hasMtpTwin(volume, volumes)) return volume.name
  return tString('adb.volumeLabelWithSuffix', { deviceName: volume.name })
}
