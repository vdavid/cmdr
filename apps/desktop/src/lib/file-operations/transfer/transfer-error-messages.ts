/**
 * User-friendly error message generation for transfer (copy/move) operations.
 * Extracted from TransferErrorDialog.svelte for testability.
 *
 * Error classification happens on the backend — each WriteOperationError variant
 * carries structured data, so the frontend just maps variant → user-facing text.
 * No string parsing needed.
 */

import type { WriteOperationError, TransferOperationType } from '$lib/file-explorer/types'
import { formatBytes } from '$lib/tauri-commands'
import { isMacOS } from '$lib/shortcuts/key-capture'

export interface FriendlyErrorMessage {
  /** Short title for the error */
  title: string
  /** Main explanation of what happened */
  message: string
  /** Suggestion for what the user can do */
  suggestion: string
}

const operationVerbMap: Record<TransferOperationType, { verb: string; pastTense: string; gerund: string }> = {
  copy: { verb: 'copy', pastTense: 'copied', gerund: 'copying' },
  move: { verb: 'move', pastTense: 'moved', gerund: 'moving' },
  delete: { verb: 'delete', pastTense: 'deleted', gerund: 'deleting' },
  trash: { verb: 'move to trash', pastTense: 'moved to trash', gerund: 'moving to trash' },
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

  switch (error.type) {
    case 'source_not_found':
      return {
        title: "Couldn't find the file",
        message: `The file or folder you tried to ${verb} no longer exists.`,
        suggestion: 'It may have been moved, renamed, or deleted. Try refreshing the file list.',
      }
    case 'destination_exists':
      return {
        title: 'File already exists',
        message: "There's already a file with this name at the destination.",
        suggestion: 'Choose a different name or location, or delete the existing file first.',
      }
    case 'permission_denied': {
      const isDeleteOp = operationType === 'delete' || operationType === 'trash'
      return {
        title: "Couldn't access this location",
        message: `You don't have permission to ${verb} files here.`,
        suggestion: isDeleteOp
          ? isMacOS()
            ? 'Check that you have write access to the parent folder. The file may be locked — unlock it in Finder (Get Info > uncheck Locked) and try again.'
            : 'Check that you have write access to the parent folder. The file may be protected — check its permissions (e.g. via chmod or your file manager) and try again.'
          : 'Check that you have write access to the destination folder. You may need to unlock the device or change folder permissions.',
      }
    }
    case 'insufficient_space':
      return {
        title: 'Not enough space',
        message: `The destination needs ${formatBytes(error.required)} but only has ${formatBytes(error.available)} available.`,
        suggestion:
          'Free up some space on the destination by deleting unnecessary files, or choose a different location.',
      }
    case 'same_location':
      return {
        title: `Can't ${verb} to the same location`,
        message: 'The source and destination are the same.',
        suggestion: 'Choose a different destination folder.',
      }
    case 'destination_inside_source':
      return {
        title: `Can't ${verb} a folder into itself`,
        message: `You're trying to ${verb} a folder into one of its own subfolders.`,
        suggestion: `Choose a destination outside of the folder you are ${gerund}.`,
      }
    case 'symlink_loop':
      return {
        title: 'Link loop detected',
        message: 'This folder contains symbolic links that create an infinite loop.',
        suggestion: 'The folder structure contains circular references. You may need to remove some symbolic links.',
      }
    case 'cancelled':
      return {
        title: `${Verb} cancelled`,
        message: `The ${verb} operation was cancelled.`,
        suggestion: 'You can try again when ready.',
      }
    case 'device_disconnected':
      return {
        title: 'Device disconnected',
        message: `The device was disconnected during the ${verb}.`,
        suggestion: 'Make sure the device is properly connected and try again.',
      }
    case 'read_only_device':
      return {
        title: 'Read-only device',
        message: `${error.deviceName ?? 'The target device'} is read-only. You can copy files from it, but not to it.`,
        suggestion: 'Choose a different destination that supports writing.',
      }
    case 'file_locked':
      return {
        title: "File is locked",
        message: "The file is locked and can't be deleted.",
        suggestion: isMacOS()
          ? 'Unlock it in Finder (Get Info > uncheck Locked) and try again.'
          : 'The file may be protected — check its permissions (e.g. via chmod or your file manager) and try again.',
      }
    case 'trash_not_supported':
      return {
        title: "Trash not supported",
        message: "This volume doesn't support trash.",
        suggestion: 'Use Shift+F8 to delete permanently instead.',
      }
    case 'connection_interrupted':
      return {
        title: 'Connection interrupted',
        message: 'The connection was interrupted.',
        suggestion:
          'Check your connection and try again. If copying to a network location, ensure the server is reachable.',
      }
    case 'read_error':
      return {
        title: `${Verb} failed`,
        message: "Couldn't read from the source.",
        suggestion: 'Try again. If the problem persists, check the technical details below.',
      }
    case 'write_error':
      return {
        title: `${Verb} failed`,
        message: "Couldn't write to the destination.",
        suggestion: 'Try again. If the problem persists, check the technical details below.',
      }
    case 'name_too_long':
      return {
        title: 'Name too long',
        message: 'The file name is too long for the destination.',
        suggestion: 'Try renaming the file to use a shorter name.',
      }
    case 'invalid_name':
      return {
        title: 'Invalid file name',
        message: 'The file name contains characters not allowed at the destination.',
        suggestion: 'Try renaming the file to remove special characters.',
      }
    case 'io_error':
      return {
        title: `${Verb} failed`,
        message: `Couldn't ${verb} the file.`,
        suggestion: 'Try again. If the problem persists, check the technical details below.',
      }
    default:
      return {
        title: `${Verb} failed`,
        message: `An unexpected error occurred while ${gerund}.`,
        suggestion: 'Try again, or check the technical details below for more information.',
      }
  }
}

/**
 * Returns the technical details for an error (path, raw error message, etc.)
 */
export function getTechnicalDetails(error: WriteOperationError): string {
  const lines: string[] = []

  switch (error.type) {
    case 'source_not_found':
    case 'destination_exists':
    case 'same_location':
    case 'symlink_loop':
    case 'device_disconnected':
    case 'file_locked':
    case 'trash_not_supported':
    case 'connection_interrupted':
    case 'name_too_long':
      lines.push(`Path: ${error.path}`)
      break
    case 'read_only_device':
      lines.push(`Path: ${error.path}`)
      if (error.deviceName) {
        lines.push(`Device: ${error.deviceName}`)
      }
      break
    case 'permission_denied':
      lines.push(`Path: ${error.path}`)
      if (error.message) {
        lines.push(`Details: ${error.message}`)
      }
      break
    case 'insufficient_space':
      lines.push(`Required: ${formatBytes(error.required)}`)
      lines.push(`Available: ${formatBytes(error.available)}`)
      if (error.volumeName) {
        lines.push(`Volume: ${error.volumeName}`)
      }
      break
    case 'destination_inside_source':
      lines.push(`Source: ${error.source}`)
      lines.push(`Destination: ${error.destination}`)
      break
    case 'cancelled':
      if (error.message) {
        lines.push(`Details: ${error.message}`)
      }
      break
    case 'read_error':
    case 'write_error':
    case 'invalid_name':
    case 'io_error':
      lines.push(`Path: ${error.path}`)
      lines.push(`Error: ${error.message}`)
      break
  }

  lines.push(`Error type: ${error.type}`)

  return lines.join('\n')
}
