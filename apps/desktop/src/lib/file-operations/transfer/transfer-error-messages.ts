/**
 * User-friendly error message generation for transfer (copy/move) operations.
 * Extracted from TransferErrorDialog.svelte for testability.
 *
 * Error classification happens on the backend: each WriteOperationError variant
 * carries structured data, so the frontend just maps variant → user-facing text.
 * No string parsing needed.
 *
 * The literal English lives in the `errors.write.*` message catalog and is pulled
 * via `getMessage()` (a RAW catalog lookup, never ICU `t()`): these strings carry
 * interpolated paths/sizes/HTML escaping and bypass ICU's brace/apostrophe
 * grammar (so the catalog values use normal apostrophes, not doubled). This
 * file keeps the COMPOSITION (verb selection, escaping, size colorizing,
 * platform branches) and substitutes `{verb}`/`{Verb}`/`{gerund}` and the
 * per-variant tokens into the catalog templates. See `$lib/intl`'s docs.
 */

import type { WriteOperationError, TransferOperationType, FriendlyError } from '$lib/file-explorer/types'
import { formatBytes } from '$lib/tauri-commands'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { colorizeSizeString } from '$lib/file-explorer/selection/selection-info-utils'
import { escapeHtml } from '$lib/tooltip/tooltip'
import { getMessage } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/** Substitutes `{token}` placeholders in a catalog value with runtime strings. */
function interpolate(template: string, params: Record<string, string> = {}): string {
  let out = template
  for (const [name, value] of Object.entries(params)) {
    out = out.replaceAll(`{${name}}`, value)
  }
  return out
}

/** Raw catalog lookup for an `errors.write.*` key (no ICU). */
function w(key: string, params?: Record<string, string>): string {
  const value = getMessage(`errors.write.${key}` as MessageKey)
  return params ? interpolate(value, params) : value
}

export interface FriendlyErrorMessage {
  /** Short title for the error */
  title: string
  /** Main explanation of what happened */
  message: string
  /** Suggestion for what the user can do */
  suggestion: string
}

const operationVerbMap: Record<TransferOperationType, { verb: string; gerund: string }> = {
  copy: { verb: 'copy', gerund: 'copying' },
  move: { verb: 'move', gerund: 'moving' },
  delete: { verb: 'delete', gerund: 'deleting' },
  trash: { verb: 'move to trash', gerund: 'moving to trash' },
}

interface OperationVerbs {
  verb: string
  Verb: string
  gerund: string
}

/** Simple error messages that only depend on the operation verbs, not on error-specific fields. */
const simpleMessageFactories: Partial<
  Record<WriteOperationError['type'], (v: OperationVerbs) => FriendlyErrorMessage>
> = {
  source_not_found: ({ verb }) => ({
    title: w('sourceNotFound.title'),
    message: w('sourceNotFound.message', { verb }),
    suggestion: w('sourceNotFound.suggestion'),
  }),
  destination_exists: () => ({
    title: w('destinationExists.title'),
    message: w('destinationExists.message'),
    suggestion: w('destinationExists.suggestion'),
  }),
  same_location: ({ verb }) => ({
    title: w('sameLocation.title', { verb }),
    message: w('sameLocation.message'),
    suggestion: w('sameLocation.suggestion'),
  }),
  destination_inside_source: ({ verb, gerund }) => ({
    title: w('destinationInsideSource.title', { verb }),
    message: w('destinationInsideSource.message', { verb }),
    suggestion: w('destinationInsideSource.suggestion', { gerund }),
  }),
  symlink_loop: () => ({
    title: w('symlinkLoop.title'),
    message: w('symlinkLoop.message'),
    suggestion: w('symlinkLoop.suggestion'),
  }),
  cancelled: ({ verb, Verb }) => ({
    title: w('cancelled.title', { Verb }),
    message: w('cancelled.message', { verb }),
    suggestion: w('cancelled.suggestion'),
  }),
  device_disconnected: ({ verb }) => ({
    title: w('deviceDisconnected.title'),
    message: w('deviceDisconnected.message', { verb }),
    suggestion: w('deviceDisconnected.suggestion'),
  }),
  trash_not_supported: () => {
    // Interpolate the live `file.deletePermanently` binding (platform-formatted)
    // at message-build time. Snapshot semantics are right here: a transient error
    // string isn't a live-updating surface. Falls back to the default if unbound.
    const deletePermanentlyKey = getEffectiveShortcuts('file.deletePermanently')[0] ?? (isMacOS() ? '⇧F8' : 'Shift+F8')
    return {
      title: w('trashNotSupported.title'),
      message: w('trashNotSupported.message'),
      suggestion: w('trashNotSupported.suggestion', { deletePermanentlyKey }),
    }
  },
  connection_interrupted: () => ({
    title: w('connectionInterrupted.title'),
    message: w('connectionInterrupted.message'),
    suggestion: w('connectionInterrupted.suggestion'),
  }),
  read_error: ({ Verb }) => ({
    title: w('readError.title', { Verb }),
    message: w('readError.message'),
    suggestion: w('readError.suggestion'),
  }),
  write_error: ({ Verb }) => ({
    title: w('writeError.title', { Verb }),
    message: w('writeError.message'),
    suggestion: w('writeError.suggestion'),
  }),
  name_too_long: () => ({
    title: w('nameTooLong.title'),
    message: w('nameTooLong.message'),
    suggestion: w('nameTooLong.suggestion'),
  }),
  invalid_name: () => ({
    title: w('invalidName.title'),
    message: w('invalidName.message'),
    suggestion: w('invalidName.suggestion'),
  }),
  io_error: ({ verb, Verb }) => ({
    title: w('ioError.title', { Verb }),
    message: w('ioError.message', { verb }),
    suggestion: w('ioError.suggestion'),
  }),
}

/** Drives the dialog's icon, container tint, and Retry-button visibility. */
export interface ErrorDisplayMeta {
  category: FriendlyError['category']
  retryHint: boolean
}

/**
 * Per-variant category + retryHint, mirrored verbatim from the values the Rust
 * write-error mapper assigned per `WriteOperationError` variant. The backend
 * ships only the typed variant; this is the FE side of that classification. The
 * dialog renders a Retry button when the category is `transient` or `retryHint`
 * is true. A `Record` keyed by every variant makes adding a variant a compile
 * error here, keeping the table exhaustive.
 *
 * `device_disconnected` keeps `retryHint: true` for the operation dialog (retry
 * the move/copy after reconnecting), unlike the listing path which shows no Retry.
 */
const errorDisplayMetaMap: Record<WriteOperationError['type'], ErrorDisplayMeta> = {
  cancelled: { category: 'transient', retryHint: true },
  connection_interrupted: { category: 'transient', retryHint: true },
  delete_pending: { category: 'transient', retryHint: true },
  device_disconnected: { category: 'needs_action', retryHint: true },
  read_error: { category: 'serious', retryHint: true },
  write_error: { category: 'serious', retryHint: true },
  io_error: { category: 'serious', retryHint: true },
  symlink_loop: { category: 'serious', retryHint: false },
  source_not_found: { category: 'needs_action', retryHint: false },
  same_location: { category: 'needs_action', retryHint: false },
  destination_exists: { category: 'needs_action', retryHint: false },
  permission_denied: { category: 'needs_action', retryHint: false },
  insufficient_space: { category: 'needs_action', retryHint: false },
  destination_inside_source: { category: 'needs_action', retryHint: false },
  read_only_device: { category: 'needs_action', retryHint: false },
  file_locked: { category: 'needs_action', retryHint: false },
  trash_not_supported: { category: 'needs_action', retryHint: false },
  name_too_long: { category: 'needs_action', retryHint: false },
  invalid_name: { category: 'needs_action', retryHint: false },
}

export function getErrorDisplayMeta(error: WriteOperationError): ErrorDisplayMeta {
  return errorDisplayMetaMap[error.type]
}

/**
 * Returns a user-friendly message for a transfer operation error.
 * Volume-agnostic: doesn't mention MTP, SMB, etc. directly.
 */
export function getUserFriendlyMessage(
  error: WriteOperationError,
  operationType: TransferOperationType = 'copy',
): FriendlyErrorMessage {
  const { verb, gerund } = operationVerbMap[operationType]
  const Verb = verb.charAt(0).toUpperCase() + verb.slice(1)
  const verbs: OperationVerbs = { verb, Verb, gerund }

  const simpleFactory = simpleMessageFactories[error.type]
  if (simpleFactory) return simpleFactory(verbs)

  switch (error.type) {
    case 'permission_denied': {
      const isDeleteOp = operationType === 'delete' || operationType === 'trash'
      return {
        title: w('permissionDenied.title'),
        // allowed-pluralize-noun: `verb` is an action name (copy/move/delete), not a count.
        message: w('permissionDenied.message', { verb }),
        suggestion: isDeleteOp
          ? isMacOS()
            ? w('permissionDenied.suggestion.deleteMac')
            : w('permissionDenied.suggestion.deleteOther')
          : w('permissionDenied.suggestion.default'),
      }
    }
    case 'insufficient_space':
      return {
        title: w('insufficientSpace.title'),
        message: w('insufficientSpace.message', {
          required: colorizeSizeString(formatBytes(error.required)),
          available: colorizeSizeString(formatBytes(error.available)),
        }),
        suggestion: w('insufficientSpace.suggestion'),
      }
    case 'read_only_device':
      return {
        title: w('readOnlyDevice.title'),
        message: w('readOnlyDevice.message', {
          deviceName: escapeHtml(error.deviceName ?? w('readOnlyDevice.fallbackName')),
        }),
        suggestion: w('readOnlyDevice.suggestion'),
      }
    case 'file_locked':
      return {
        title: w('fileLocked.title'),
        message: w('fileLocked.message'),
        suggestion: isMacOS() ? w('fileLocked.suggestion.mac') : w('fileLocked.suggestion.other'),
      }
    case 'delete_pending':
      // STATUS_DELETE_PENDING: the file is marked for deletion on the server but
      // an open handle is keeping it alive. Transient: retry-after-a-moment.
      // Mirrors the prose the Rust write_error path produced (kinds::delete_pending).
      return {
        title: w('deletePending.title'),
        message: w('deletePending.message'),
        suggestion: w('deletePending.suggestion'),
      }
    default:
      return {
        title: w('fallback.title', { Verb }),
        message: w('fallback.message', { gerund }),
        suggestion: w('fallback.suggestion'),
      }
  }
}

/** Error types where technical details are just the path. */
const pathOnlyTypes = new Set<WriteOperationError['type']>([
  'source_not_found',
  'destination_exists',
  'same_location',
  'symlink_loop',
  'device_disconnected',
  'file_locked',
  'trash_not_supported',
  'connection_interrupted',
  'name_too_long',
  'delete_pending',
])

/** Error types where technical details include path + error message. */
const pathAndMessageTypes = new Set<WriteOperationError['type']>([
  'read_error',
  'write_error',
  'invalid_name',
  'io_error',
])

/**
 * Returns the technical details for an error (path, raw error message, etc.)
 */
export function getTechnicalDetails(error: WriteOperationError): string {
  const lines: string[] = []

  if (pathOnlyTypes.has(error.type)) {
    lines.push(`Path: ${(error as { path: string }).path}`)
  } else if (pathAndMessageTypes.has(error.type)) {
    lines.push(`Path: ${(error as { path: string }).path}`)
    lines.push(`Error: ${(error as { message: string }).message}`)
  } else if (error.type === 'read_only_device') {
    lines.push(`Path: ${error.path}`)
    if (error.deviceName) lines.push(`Device: ${error.deviceName}`)
  } else if (error.type === 'permission_denied') {
    lines.push(`Path: ${error.path}`)
    if (error.message) lines.push(`Details: ${error.message}`)
  } else if (error.type === 'insufficient_space') {
    lines.push(`Required: ${formatBytes(error.required)}`)
    lines.push(`Available: ${formatBytes(error.available)}`)
    if (error.volumeName) lines.push(`Volume: ${error.volumeName}`)
  } else if (error.type === 'destination_inside_source') {
    lines.push(`Source: ${error.source}`)
    lines.push(`Destination: ${error.destination}`)
  } else if (error.type === 'cancelled') {
    if (error.message) lines.push(`Details: ${error.message}`)
  }

  lines.push(`Error type: ${error.type}`)

  return lines.join('\n')
}
