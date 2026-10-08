/**
 * Which volume a pane is on, for its header, the switcher's checkmark, and the
 * first-connect index prompt.
 *
 * Two answers exist. The pane's own `volumeId` names the volume it was put on,
 * and `containingVolumeId` is what the backend says holds its PATH. Neither is
 * right everywhere:
 *
 * - A favorite's id is virtual, and a local pane can walk into another drive
 *   (`/Volumes/USB` from Macintosh HD), so for those the path decides.
 * - ❗ A share, a server place, or a phone is ON its volume by id. While an ejected
 *   share comes back, `/Volumes/<share>` is a plain folder on the boot disk, so
 *   the path answered "Macintosh HD", and a LIVE share is listed as an attached
 *   volume, not `network` (QA rounds 3 and 4, N1).
 */

import type { VolumeInfo } from '../types'

/** Categories whose id is never the pane's answer: virtual, or wide enough to hold other drives. */
const PATH_DECIDES = new Set<VolumeInfo['category']>(['favorite', 'main_volume', 'cloud_drive'])

/**
 * The volume the pane is on: its own `volumeId` when that is a real place the
 * pane's path sits in, else the one holding the path.
 */
export function paneVolumeOf(
  volumes: readonly VolumeInfo[],
  volumeId: string,
  currentPath: string,
  containingVolumeId: string | null,
): VolumeInfo | undefined {
  const own = volumes.find((v) => v.id === volumeId)
  if (own && !PATH_DECIDES.has(own.category) && (ownsById(own) || isAtOrUnder(currentPath, own.path))) return own
  return volumes.find((v) => v.id === containingVolumeId)
}

/** The volume mounted at exactly `path`, for events that name a mount only by path. */
export function volumeMountedAt(volumes: readonly VolumeInfo[], path: string): VolumeInfo | undefined {
  // ❗ A favorite may point at a mount root, and favorites list first.
  return volumes.find((v) => v.category !== 'favorite' && v.path === path)
}

/** A phone or a saved server place has no mount the path could be checked against. */
function ownsById(volume: VolumeInfo): boolean {
  return volume.category === 'mobile_device' || volume.category === 'network'
}

/** Whether `path` is `root` or inside it, by whole components. */
function isAtOrUnder(path: string, root: string): boolean {
  const base = root.endsWith('/') ? root : `${root}/`
  return path === root || path.startsWith(base)
}
