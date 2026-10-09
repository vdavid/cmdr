/**
 * Which kind of identity a volume id was minted from, read off its shape.
 *
 * The frontend twin of `VolumeScheme::of` in `crates/cmdr-fs/src/volume/ids.rs`,
 * and the ONLY place in the frontend that reads a volume id's prefix. Keep the
 * two in step: a scheme added there is added here.
 *
 * Shape-only: an `'smb'` answer says the id was minted for an SMB share, NOT
 * that the share is connected or even still known. Ask the volume store for
 * liveness.
 *
 * Where a caller makes a per-kind decision, `switch` over the result with an
 * exhaustive `never` default, so a new scheme fails the typecheck instead of
 * silently falling into some other kind's branch.
 */
export type VolumeScheme =
  | 'root'
  | 'local'
  | 'path'
  | 'smb'
  | 'sftp'
  | 'webdav'
  | 's3'
  | 'mtp'
  | 'adb'
  | 'cloud'
  | 'favorite'
  | 'unknown'

/** The `{tag}-` prefix each prefixed scheme is minted under. `root` is a bare literal. */
const PREFIXES: ReadonlyArray<readonly [string, Exclude<VolumeScheme, 'root' | 'unknown'>]> = [
  ['vol-', 'local'],
  ['path-', 'path'],
  ['smb-', 'smb'],
  ['sftp-', 'sftp'],
  ['webdav-', 'webdav'],
  ['s3-', 's3'],
  ['mtp-', 'mtp'],
  ['adb-', 'adb'],
  ['cloud-', 'cloud'],
  ['fav-', 'favorite'],
]

/**
 * Classifies a volume id by shape: `root` exactly, else its `{tag}-` prefix,
 * else `'unknown'` (which covers the `'network'` and `'search-results'` virtual
 * ids). MTP device ids (`mtp-…`) and their storage volume ids
 * (`mtp-…:{storageId}`) are both `'mtp'`.
 */
export function volumeScheme(volumeId: string): VolumeScheme {
  if (volumeId === 'root') return 'root'
  for (const [prefix, scheme] of PREFIXES) {
    // A bare tag (`adb-`) names nothing, as on the Rust side.
    if (volumeId.length > prefix.length && volumeId.startsWith(prefix)) return scheme
  }
  return 'unknown'
}
