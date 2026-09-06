import type { VolumeInfo } from '../types'
import { isLiveSession, showsDisconnect } from './connection-state'

/**
 * Whether the volume picker offers this row an eject-or-disconnect control at all.
 *
 * `isEjectable` (from NSURL on macOS, sysfs removable bit on Linux) covers USB
 * drives, SD cards, DMG-mounted disk images, and MTP devices. It returns `false`
 * for SMB mounts even though Finder shows an eject button for them, so the row's
 * session answers for those.
 *
 * The session half is two predicates because the control's WORD differs: a live
 * mount or session ends with Eject or Disconnect (`direct`, `os_mount`), and a
 * dropped-but-registered one still has a session to close (`disconnected`).
 * ❌ Neither covers `saved` (a greyed row that was never connected) or the two
 * sign-in states (nothing is open), and under the old `!= null` test all three
 * would have offered a control with no subject.
 *
 * Cloud drives, favorites, and the root volume are never ejectable on either path.
 */
export function isVolumeEjectable(volume: VolumeInfo | undefined): boolean {
  if (!volume) return false
  return volume.isEjectable || isLiveSession(volume.connectionState) || showsDisconnect(volume.connectionState)
}
