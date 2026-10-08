/**
 * The "Technical details" block of the transfer error dialog: the raw facts (paths, errno, sizes) a bug report
 * needs and the prose in `transfer-error-messages.ts` deliberately never states. English only, never translated:
 * it's for the report, not the reader.
 */
import type { WriteOperationError } from '$lib/file-explorer/types'
import { formatByteSize } from '$lib/units'

/** Error types where technical details are just the path. */
const pathOnlyTypes = new Set<WriteOperationError['type']>([
  'source_not_found',
  'destination_not_found',
  'destination_not_a_folder',
  'source_not_connected',
  'destination_not_connected',
  'source_no_longer_connected',
  'destination_exists',
  'symlink_loop',
  'file_locked',
  'trash_not_supported',
  'connection_interrupted',
  'name_too_long',
  'delete_pending',
  'destination_not_writable',
  'destination_full',
])

/** Error types where technical details include path + error message. */
const pathAndMessageTypes = new Set<WriteOperationError['type']>([
  'read_error',
  'write_error',
  'invalid_name',
  'io_error',
])

/**
 * The technical lines for a refused write.
 *
 * The errno is what a bug report needs and the prose deliberately never states:
 * `EACCES` is a folder an administrator could write to, `EPERM` is macOS refusing
 * outright. `Refused by` is the folder the backend PROVED with `access(W_OK)`, so
 * a report says which end of a move said no. Split out to keep
 * `variantDetailLines` under the complexity ceiling.
 */
function permissionDeniedDetailLines(error: Extract<WriteOperationError, { type: 'permission_denied' }>): string[] {
  const lines = [`Path: ${error.path}`]
  if (error.refusedFolder) lines.push(`Refused by: ${error.refusedFolder}`)
  if (error.errno !== null) lines.push(`Errno: ${String(error.errno)} (${error.refusal})`)
  if (error.message) lines.push(`Details: ${error.message}`)
  return lines
}

/**
 * The technical lines for the variants the two sets above don't cover.
 *
 * Split out of `getTechnicalDetails` so that stays a three-way dispatcher: this
 * chain grows one arm per new variant, and its length is the shape of the error
 * union rather than of the function.
 */
function variantDetailLines(error: WriteOperationError): string[] {
  if (error.type === 'read_only_device') {
    return error.deviceName ? [`Path: ${error.path}`, `Device: ${error.deviceName}`] : [`Path: ${error.path}`]
  }
  if (error.type === 'permission_denied') {
    return permissionDeniedDetailLines(error)
  }
  if (error.type === 'insufficient_space') {
    const lines = [`Required: ${formatByteSize(error.required)}`, `Available: ${formatByteSize(error.available)}`]
    if (error.volumeName) lines.push(`Volume: ${error.volumeName}`)
    return lines
  }
  if (error.type === 'destination_inside_source') {
    return [`Source: ${error.source}`, `Destination: ${error.destination}`]
  }
  if (error.type === 'duplicate_source_names') {
    return [`Name: ${error.name}`, `First: ${error.first}`, `Second: ${error.second}`]
  }
  if (error.type === 'files_too_large_for_filesystem') {
    return [
      `Filesystem: ${error.filesystem}`,
      `Max file size: ${formatByteSize(error.maxSize)}`,
      `Files over the limit: ${String(error.totalCount)}`,
      ...error.files.map((file) => `  ${file.name} (${formatByteSize(file.size)})`),
    ]
  }
  // Both paths, because the whole point of this variant is that the user's new
  // file is at the second one and nowhere else.
  if (error.type === 'new_data_kept_at') {
    return [`Path: ${error.path}`, `New data kept at: ${error.keptAt}`, `Error: ${error.message}`]
  }
  // Every renamed file, because the message only names one of them, and the
  // details block is the only place a user with several can find the rest. The
  // cause's own lines follow, so a bug report still carries what stopped the copy.
  if (error.type === 'originals_kept_aside') {
    return [
      ...error.recovered.map((entry) => `Kept ${entry.path} at: ${entry.keptAt}`),
      ...getTechnicalDetails(error.cause).split('\n'),
    ]
  }
  // Both ends, since the item is at both, then what refused the delete.
  if (error.type === 'source_not_removed') {
    return [
      `Original kept at: ${error.path}`,
      `Copy landed at: ${error.landedAt}`,
      ...getTechnicalDetails(error.cause).split('\n'),
    ]
  }
  if (error.type === 'cancelled' && error.message) {
    return [`Details: ${error.message}`]
  }
  return []
}

/**
 * The details for the two variants a vanished drive raises, or `null` for
 * everything else.
 *
 * The drive's identity is the thing worth having here: the volume list has
 * already dropped it, so this block is the only place its name and id survive a
 * bug report. A backend disconnect with no typed side (MTP, SMB) keeps the plain
 * path line it has always had. Split out so `variantDetailLines` stays within
 * its complexity ceiling.
 */
function driveDetailLines(error: WriteOperationError): string[] | null {
  if (error.type === 'device_disconnected') {
    return error.side
      ? [`Path: ${error.path}`, `Volume: ${error.side.volumeName} (${error.side.volumeId})`, `Side: ${error.side.role}`]
      : [`Path: ${error.path}`]
  }
  if (error.type === 'move_not_confirmed') {
    const lines = [`Path: ${error.path}`]
    if (error.errno !== null) lines.push(`Errno: ${String(error.errno)}`)
    if (error.volumeName) lines.push(`Volume: ${error.volumeName}`)
    return lines
  }
  return null
}

/**
 * The `Path:` line, or nothing for a failure that names no item (a crashed task,
 * an empty selection): a blank `Path:` reads as a path that got lost.
 */
function pathLine(path: string): string[] {
  return path ? [`Path: ${path}`] : []
}

/**
 * Returns the technical details for an error (path, raw error message, etc.)
 */
export function getTechnicalDetails(error: WriteOperationError): string {
  const lines: string[] = []
  const drive = driveDetailLines(error)

  if (drive) {
    lines.push(...drive)
  } else if (pathOnlyTypes.has(error.type)) {
    lines.push(...pathLine((error as { path: string }).path))
  } else if (pathAndMessageTypes.has(error.type)) {
    lines.push(...pathLine((error as { path: string }).path))
    lines.push(`Error: ${(error as { message: string }).message}`)
  } else {
    lines.push(...variantDetailLines(error))
  }

  lines.push(`Error type: ${error.type}`)

  return lines.join('\n')
}
