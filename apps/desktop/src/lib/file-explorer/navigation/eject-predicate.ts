import type { VolumeInfo } from '../types'
import { isLiveSession, showsDisconnect } from './connection-state'
import { isAdbVolumeId } from '$lib/adb/adb-path-utils'

/**
 * Whether the volume picker offers this row an eject-or-disconnect control at all.
 *
 * `isEjectable` (from NSURL on macOS, sysfs removable bit on Linux) covers USB
 * drives, SD cards, DMG-mounted disk images, and MTP devices. It returns `false`
 * for SMB mounts even though Finder shows an eject button for them, so the row's
 * session answers for those.
 *
 * The session half is two predicates because the control's WORD differs: a live
 * mount or session ends with Eject or Disconnect (`direct`, `os_mount`), and
 * everything else that is REGISTERED still has one to close (`disconnected` plus
 * both sign-in states). ❌ Neither covers `saved`, a greyed row that was never
 * connected, and under the old `!= null` test that one would have offered a
 * control with no subject.
 *
 * ❗ **A PHONE is answered by its READINESS, ❌ never by either of those.** Its
 * row carries `isEjectable: true` unconditionally (`device_volumes.rs`) and its
 * `connectionState` is always `null` by design (readiness is PRESENCE, never
 * session health), so the first half says yes to every device row — including a
 * greyed `unavailable` one the user cannot even open, and one still showing its
 * "Allow USB debugging?" prompt with nothing yet to disconnect.
 *
 * Cloud drives, favorites, and the root volume are never ejectable on either path.
 */
export function isVolumeEjectable(volume: VolumeInfo | undefined): boolean {
  if (!volume) return false
  if (isAdbVolumeId(volume.id)) return volume.deviceReadiness?.kind === 'ready'
  return volume.isEjectable || isLiveSession(volume.connectionState) || showsDisconnect(volume.connectionState)
}
