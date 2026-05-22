import type { VolumeInfo } from '../types'

/**
 * Whether a volume can be ejected from the volume picker.
 *
 * `isEjectable` (from NSURL on macOS, sysfs removable bit on Linux) covers USB
 * drives, SD cards, DMG-mounted disk images, and MTP devices. It returns `false`
 * for SMB mounts even though Finder shows an eject button for them, so we OR in
 * any SMB connection state. Cloud drives, favorites, and the root volume are
 * never ejectable on either path.
 */
export function isVolumeEjectable(volume: VolumeInfo | undefined): boolean {
  if (!volume) return false
  // `smbConnectionState` is typed `SmbConnectionState | undefined` but Rust's
  // `Option::None` serializes to `null`, so use `!= null` (covers both undefined
  // and null) rather than `!== undefined`.
  // eslint-disable-next-line eqeqeq -- explicit `!= null` lets us match both null and undefined in one check
  return volume.isEjectable || volume.smbConnectionState != null
}
