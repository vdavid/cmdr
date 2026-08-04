// Resolve which volume the Search dialog's image-OCR grid should target: the
// volume the user is contextually searching, i.e. the focused pane's current
// volume. Browsing the local disk searches the local index; browsing a NAS
// searches that NAS's index, so a user on their network drive finds its photos.
//
// The media-index volume id equals the pane's volume id (`root` for the local
// disk, `smb-…` for an SMB share) — they're the same identifier the indexing
// subsystem keys on. This helper looks that id up in the live volume list to
// also recover the mount root (for turning an index-relative OCR hit into an
// openable OS path via `resolveMediaHitPath`) and whether it's a network volume
// (which switches `ImageSearchResults`' coverage-honesty copy to the network
// voice).

import type { VolumeInfo } from '$lib/file-explorer/types'
import { ROOT_VOLUME_ID } from '$lib/indexing'

/** The volume the image-OCR grid targets, plus what it needs to resolve + voice hits. */
export interface ImageSearchVolume {
  /** The media-index volume id to search (== the pane's volume id). */
  volumeId: string
  /**
   * The volume's mount root, prepended to index-relative hit paths. `/` for the
   * local root (hits are already absolute); `/Volumes/<share>` for an SMB volume.
   */
  mountRoot: string
  /** Whether this is a network (SMB) volume, driving the network coverage voice. */
  isNetwork: boolean
}

/**
 * Resolve the image-search target for `focusedVolumeId` against the live volume
 * list. Falls back to the local root volume when the focused pane's volume isn't
 * a real filesystem volume in the list (a `search-results://` snapshot pane, or a
 * volume that has since unmounted): a virtual volume has no `media.db`, so the
 * local index is the sensible default.
 */
export function resolveImageSearchVolume(volumes: VolumeInfo[], focusedVolumeId: string): ImageSearchVolume {
  const info = volumes.find((v) => v.id === focusedVolumeId)
  if (!info) {
    return { volumeId: ROOT_VOLUME_ID, mountRoot: '/', isNetwork: false }
  }
  return {
    volumeId: info.id,
    mountRoot: info.path,
    isNetwork: info.category === 'network',
  }
}
