// How an expanded queue row writes one path. The one place to change the rule.

/** Between a volume's name and a path on it. The same ` › ` the settings
 *  breadcrumbs use (`settings-search.ts`), so it reads as "inside". Not a
 *  catalog string: it's punctuation, the same in every language. */
const VOLUME_PATH_SEPARATOR = ' › '

/**
 * A path as the queue row's details panel shows it.
 *
 * The rule: a path the Mac itself can resolve (the local disk, a mounted
 * drive, an OS-mounted share) names its place on its own, so it's shown as is.
 * A path on a volume the OS can't see (an MTP phone, an S3 bucket, an SFTP or
 * WebDAV server) is relative to that volume, so `/DCIM/x` alone could be
 * anywhere; it gets the volume's name in front: `Pixel 8 › /DCIM/x`.
 *
 * The backend makes the call per side and sends the name only when it applies
 * (`OperationDetails.sourceVolumeName` / `destinationVolumeName`, from
 * `OperationPaths::volume_label`), using the same volume name the row's
 * summary shows. This function only joins the two.
 */
export function formatOperationPath(path: string, volumeName: string | null): string {
  return volumeName ? `${volumeName}${VOLUME_PATH_SEPARATOR}${path}` : path
}
