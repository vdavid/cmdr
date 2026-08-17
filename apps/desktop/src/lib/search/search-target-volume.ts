// The ONE volume a Search dialog session covers: the focused pane's current
// volume. A search reaches at most one volume (`src-tauri/src/search/execute.rs`),
// so the same answer serves every part of the dialog that has to know which drive
// it's talking about:
//
//   - the readiness gate ("is this search's target index loaded, or is one still
//     on its way?"),
//   - the image-OCR grid (browsing the NAS surfaces its photos, browsing local
//     surfaces local; the media-index volume id IS the pane's volume id),
//   - the mount root, prepended to index-relative OCR hits to rebuild an openable
//     OS path (`resolveMediaHitPath`),
//   - and whether it's a network volume, which switches the coverage-honesty copy
//     to the network voice.

import type { VolumeInfo } from '$lib/file-explorer/types'
import { volumeKindOf } from '$lib/file-explorer/pane/volume-capabilities'
import { ROOT_VOLUME_ID } from '$lib/indexing'

/**
 * Whether a volume is a network share, by its TYPED kind rather than its category.
 *
 * A `category === 'network'` test alone misses the common case: an SMB share Cmdr
 * couldn't upgrade to a direct connection stays an OS mount under `/Volumes`, and the
 * volume list hands it back as `attached_volume` with `fsType: 'smbfs'`. Voicing that
 * as a local drive tells a NAS user their boot disk isn't indexed. `volumeKindOf` is
 * the single frontend classifier (`file-explorer/pane/CLAUDE.md`) and
 * covers both shapes.
 */
function isNetworkVolume(info: VolumeInfo): boolean {
  return volumeKindOf(info.id, info.fsType, info.category) === 'smb'
}

/** The volume a Search session covers, plus what the dialog needs to voice it. */
export interface SearchTargetVolume {
  /** The volume id the search (and the media index) targets. */
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
 * Resolve the search target for `focusedVolumeId` against the live volume list.
 * Falls back to the local root volume when the focused pane's volume isn't a real
 * filesystem volume in the list (a `search-results://` snapshot pane, or a volume
 * that has since unmounted): a virtual volume owns no index, so the local one is the
 * sensible default — the same fallback `resolveDefaultScope` makes for the scope.
 */
export function resolveSearchTargetVolume(volumes: VolumeInfo[], focusedVolumeId: string): SearchTargetVolume {
  const info = volumes.find((v) => v.id === focusedVolumeId)
  if (!info) {
    return { volumeId: ROOT_VOLUME_ID, mountRoot: '/', isNetwork: false }
  }
  return {
    volumeId: info.id,
    mountRoot: info.path,
    isNetwork: isNetworkVolume(info),
  }
}

/**
 * How a drive the user should be told about is named and voiced: its display name
 * (empty when the volume isn't in the live list — an ejected drive, or an SMB share
 * that's no longer mounted) and whether it's a network drive.
 *
 * Kept separate from `resolveSearchTargetVolume` because the volume a COVERAGE GAP
 * belongs to isn't always the pane's: a scope typed into the box can point at another
 * drive, and the backend names the volume it actually routed to
 * (`SearchResult.targetVolumeId`).
 */
export function describeVolume(volumes: VolumeInfo[], volumeId: string): { name: string; isNetwork: boolean } {
  const info = volumes.find((v) => v.id === volumeId)
  return { name: info?.name ?? '', isNetwork: info ? isNetworkVolume(info) : false }
}
