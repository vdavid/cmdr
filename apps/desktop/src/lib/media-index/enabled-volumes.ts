/**
 * The media-index volumes the settings slider previews and the reclaim UI act over: the
 * built-in local root plus every opted-in SMB volume. Non-opted-in SMB and MTP volumes
 * aren't background-enriched, so they're left out. Shared by the importance slider's
 * covered-count preview and the reclaim-space line so the two always count the same set.
 */
import { ROOT_VOLUME_ID } from '$lib/indexing'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { getNetworkOptInVolumes } from '$lib/media-index/network-volume-prefs'

/** The enabled media-index volume ids: the local root plus opted-in SMB volumes. */
export function getEnabledMediaIndexVolumeIds(): string[] {
  const optedIn = new Set(getNetworkOptInVolumes())
  const networkIds = getVolumes()
    .filter((v: VolumeInfo) => v.category === 'network' && optedIn.has(v.id))
    .map((v) => v.id)
  return [ROOT_VOLUME_ID, ...networkIds]
}
