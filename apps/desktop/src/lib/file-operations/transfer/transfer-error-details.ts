/**
 * The "Technical details" block of the transfer error dialog: the raw facts (paths, errno, sizes) a bug report
 * needs and the prose in `transfer-error-messages.ts` deliberately never states. English only, never translated:
 * it's for the report, not the reader.
 */
import type { WriteOperationError } from '#lib/file-explorer/types.js'
import { formatByteSize } from '#lib/units/index.js'

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
  'landing_not_recorded',
  'source_in_cold_storage',
  'source_changed',
])

/** Error types where technical details include path + error message. */
const pathAndMessageTypes = new Set<WriteOperationError['type']>([
  'read_error',
  'write_error',
  'invalid_name',
  'io_error',
  'reply_signature_refused',
])

type ErrorOfType<T extends WriteOperationError['type']> = Extract<WriteOperationError, { type: T }>

/** One formatter per variant the two sets above don't cover, each typed to its own variant. */
type VariantDetailFormatters = {
  [T in WriteOperationError['type']]?: (error: ErrorOfType<T>) => string[]
}

/** The lines of the error behind a wrapping variant, so a bug report still carries what stopped the transfer. */
function causeLines(cause: WriteOperationError): string[] {
  return getTechnicalDetails(cause).split('\n')
}

const variantDetailFormatters: VariantDetailFormatters = {
  read_only_device: (error) => [`Path: ${error.path}`, ...(error.deviceName ? [`Device: ${error.deviceName}`] : [])],
  // The errno is what a bug report needs and the prose deliberately never states:
  // `EACCES` is a folder an administrator could write to, `EPERM` is macOS refusing
  // outright. `Refused by` is the folder the backend PROVED with `access(W_OK)`, so
  // a report says which end of a move said no.
  permission_denied: (error) => {
    const lines = [`Path: ${error.path}`]
    if (error.refusedFolder) lines.push(`Refused by: ${error.refusedFolder}`)
    if (error.errno !== null) lines.push(`Errno: ${String(error.errno)} (${error.refusal})`)
    if (error.message) lines.push(`Details: ${error.message}`)
    return lines
  },
  insufficient_space: (error) => [
    `Required: ${formatByteSize(error.required)}`,
    `Available: ${formatByteSize(error.available)}`,
    ...(error.volumeName ? [`Volume: ${error.volumeName}`] : []),
  ],
  destination_inside_source: (error) => [`Source: ${error.source}`, `Destination: ${error.destination}`],
  mounted_volume_inside: (error) => [`Mount point: ${error.mountPoint}`],
  duplicate_source_names: (error) => [`Name: ${error.name}`, `First: ${error.first}`, `Second: ${error.second}`],
  files_too_large_for_filesystem: (error) => [
    `Filesystem: ${error.filesystem}`,
    `Max file size: ${formatByteSize(error.maxSize)}`,
    `Files over the limit: ${String(error.totalCount)}`,
    ...error.files.map((file) => `  ${file.name} (${formatByteSize(file.size)})`),
  ],
  // Both paths, because the whole point of this variant is that the user's new
  // file is at the second one and nowhere else.
  new_data_kept_at: (error) => [`Path: ${error.path}`, `New data kept at: ${error.keptAt}`, `Error: ${error.message}`],
  // Every renamed file, because the message only names one of them, and the
  // details block is the only place a user with several can find the rest.
  originals_kept_aside: (error) => [
    ...error.recovered.map((entry) => `Kept ${entry.path} at: ${entry.keptAt}`),
    ...causeLines(error.cause),
  ],
  // Every group of kept files with its count, since the message names only one
  // file per group.
  new_files_kept: (error) => [
    ...error.kept.map(
      (group) =>
        `Kept (${group.fate}) on ${group.volumeName}: ${String(group.count)}, for example ${group.exampleName}`,
    ),
    ...causeLines(error.cause),
  ],
  // Both ends, since the item is at both, then what refused the delete.
  source_not_removed: (error) => [
    `Original kept at: ${error.path}`,
    `Copy landed at: ${error.landedAt}`,
    ...causeLines(error.cause),
  ],
  cancelled: (error) => (error.message ? [`Details: ${error.message}`] : []),
  // A volume that went away: the drive's identity is the thing worth having here.
  // The volume list has already dropped it, so this block is the only place its
  // name and id survive a bug report. A backend disconnect with no typed side
  // (MTP, SMB) keeps the plain path line.
  device_disconnected: (error) => [
    `Path: ${error.path}`,
    ...(error.side ? [`Volume: ${error.side.volumeName} (${error.side.volumeId})`, `Side: ${error.side.role}`] : []),
  ],
  // The server's name and the wait, which the prose words but a report needs as
  // plain facts.
  server_connection_lost: (error) => [
    `Path: ${error.path}`,
    `Server: ${error.volumeName}`,
    `Waited: ${error.gaveUpAfterSecs === null ? 'no' : `${String(error.gaveUpAfterSecs)} s`}`,
  ],
  changed_while_disconnected: (error) => [`Path: ${error.path}`, `Volume: ${error.volumeName}`],
  move_not_confirmed: (error) => [
    `Path: ${error.path}`,
    ...(error.errno !== null ? [`Errno: ${String(error.errno)}`] : []),
    ...(error.volumeName ? [`Volume: ${error.volumeName}`] : []),
  ],
}

/**
 * The `Path:` line, or nothing for a failure that names no item (a crashed task,
 * an empty selection): a blank `Path:` reads as a path that got lost.
 */
function pathLine(path: string): string[] {
  return path ? [`Path: ${path}`] : []
}

/** The lines above `Error type:` for one error. */
function detailLines(error: WriteOperationError): string[] {
  // The table is keyed by `type`, so the formatter found is the one typed for
  // this variant; TypeScript can't correlate the two through the lookup.
  const format = variantDetailFormatters[error.type] as ((error: WriteOperationError) => string[]) | undefined
  if (format) return format(error)
  if (pathOnlyTypes.has(error.type)) return pathLine((error as { path: string }).path)
  if (pathAndMessageTypes.has(error.type)) {
    return [...pathLine((error as { path: string }).path), `Error: ${(error as { message: string }).message}`]
  }
  return []
}

/**
 * Returns the technical details for an error (path, raw error message, etc.)
 */
export function getTechnicalDetails(error: WriteOperationError): string {
  return [...detailLines(error), `Error type: ${error.type}`].join('\n')
}
