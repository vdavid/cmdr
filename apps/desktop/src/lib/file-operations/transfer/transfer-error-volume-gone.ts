/**
 * What the transfer error dialog says when a volume went away mid-transfer: a drive
 * or device that left (`device_disconnected`), and a server whose connection was
 * lost (`server_connection_lost`), plus what a delete that waited one out left alone
 * (`changed_while_disconnected`). All are about where the person's files are and
 * what to check, so they live together, split out of `transfer-error-messages.ts`.
 * DETAILS § "What the error dialog says when a drive was pulled" and § "What it says
 * when a server's connection was lost".
 */
import type { WriteOperationError, TransferOperationType } from '#lib/file-explorer/types.js'
import type { ProgressAtStop } from '#lib/tauri-commands/index.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { escapeHtml } from '#lib/tooltip/tooltip.js'
import { formatDuration, seconds } from '#lib/units/index.js'
import { w, type FriendlyErrorMessage } from './transfer-error-catalog'

/**
 * The outcomes of a volume that went away mid-transfer: a drive or device that
 * left, a server whose connection was lost, and a file a delete found changed once
 * the connection was back. `getUserFriendlyMessage` asks it before
 * its own switch. `null` means "not one of mine".
 */
export function volumeGoneMessage(
  error: WriteOperationError,
  operationType: TransferOperationType,
  progressAtStop: ProgressAtStop | null,
): FriendlyErrorMessage | null {
  switch (error.type) {
    case 'device_disconnected':
      return deviceDisconnectedMessage(error, operationType, progressAtStop)
    case 'server_connection_lost':
      return serverConnectionLostMessage(error, operationType)
    // Only a delete builds it: the file is the point here, so it's named in the title.
    case 'changed_while_disconnected':
      return {
        title: w('changedWhileDisconnected.title'),
        message: w('changedWhileDisconnected.message', {
          volumeName: escapeHtml(error.volumeName),
          fileName: escapeHtml(lastPathSegment(error.path)),
        }),
        suggestion: w('changedWhileDisconnected.suggestion'),
      }
    default:
      return null
  }
}

/**
 * The sentence for a drive that left in the middle of a transfer.
 *
 * Three facts, in the order a worried person needs them: which drive went, how
 * far the transfer got, and where their files are now. The backend names the
 * side (it captured both volumes when the transfer started, because a volume
 * that vanishes can't be looked up afterwards), so ❌ nothing here guesses one
 * from a path.
 *
 * Falls back to the volume-agnostic sentence for a backend session that dropped
 * with no typed side (MTP, SMB), and for a transfer that stopped before the
 * counts existed: a sentence reading "copied 0 of 0 files" would be worse than
 * one that doesn't count at all.
 */
function deviceDisconnectedMessage(
  error: Extract<WriteOperationError, { type: 'device_disconnected' }>,
  operationType: TransferOperationType,
  progress: ProgressAtStop | null,
): FriendlyErrorMessage {
  const title = w('deviceDisconnected.title')
  const suggestion = w('deviceDisconnected.suggestion')
  const side = error.side
  const counted = progress !== null && progress.filesTotal > 0
  // A move that stops keeps every original, so its destination sentence reports
  // no partial progress: there isn't any to report.
  const needsCounts = !(side?.role === 'destination' && operationType === 'move')
  const sidedOperation = operationType === 'copy' || operationType === 'move'
  if (side && sidedOperation && (counted || !needsCounts)) {
    return {
      title,
      message: w(`deviceDisconnected.sided.${side.role}.${operationType}`, {
        volumeName: escapeHtml(side.volumeName),
        counterpart: escapeHtml(side.counterpartName),
        done: formatInteger(progress?.filesDone ?? 0),
        total: formatInteger(progress?.filesTotal ?? 0),
      }),
      suggestion,
    }
  }
  return { title, message: w(`deviceDisconnected.message.${operationType}`), suggestion }
}

/** The operations that have their own "lost the connection" sentences, by the verb their keys use. */
const SERVER_CONNECTION_LOST_VERBS: Partial<
  Record<TransferOperationType, 'copy' | 'move' | 'delete' | 'compress' | 'archiveEdit'>
> = {
  copy: 'copy',
  move: 'move',
  delete: 'delete',
  compress: 'compress',
  archive_edit: 'archiveEdit',
}

/**
 * The sentence for a server whose connection ended a transfer.
 *
 * The connection is the cause, so the server heads the title; the file is context,
 * by name only (the full path is in the technical details). The backend's outage
 * wait decided it from typed facts, ❌ nothing here infers "server" from a path.
 * `gaveUpAfterSecs` picks between "Cmdr waited this long, then stopped" and "it
 * dropped" (no wait this time: a sign-in needed, or a link that kept dropping).
 * The verb follows the operation; one with no sentence of its own reads as a copy.
 */
function serverConnectionLostMessage(
  error: Extract<WriteOperationError, { type: 'server_connection_lost' }>,
  operationType: TransferOperationType,
): FriendlyErrorMessage {
  const verb = SERVER_CONNECTION_LOST_VERBS[operationType] ?? 'copy'
  const volumeName = escapeHtml(error.volumeName)
  const fileName = escapeHtml(lastPathSegment(error.path))
  const message =
    error.gaveUpAfterSecs === null
      ? w(`serverConnectionLost.message.dropped.${verb}`, { volumeName, fileName })
      : w(`serverConnectionLost.message.gaveUp.${verb}`, {
          volumeName,
          fileName,
          duration: formatDuration(seconds(error.gaveUpAfterSecs)),
        })
  return {
    title: w('serverConnectionLost.title', { volumeName }),
    message,
    suggestion: w(`serverConnectionLost.suggestion.${verb}`),
  }
}

/** The last segment of a backend path: the name a person recognizes the file by. */
function lastPathSegment(path: string): string {
  // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- a display name: the last non-empty name, a drive root reading as its letter
  return path.split('/').findLast((segment) => segment !== '') ?? path
}
