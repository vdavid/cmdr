/**
 * Utility functions for the transfer (copy/move) dialog. User-facing copy
 * resolves through the i18n catalog (`fileOperations.transferDialog.*`).
 */

import type { TransferOperationType } from '$lib/file-explorer/types'
import type { MessageKey } from '$lib/intl/keys.gen'
import { tString } from '$lib/intl/messages.svelte'
import { suggestCompressArchiveName } from './transfer-compress-name'

/**
 * Generates a dialog title with proper pluralization for files and folders.
 * @returns Formatted title string like "Copy 1 file", "Move 2 files and 3 folders"
 */
export function generateTitle(operationType: TransferOperationType, files: number, folders: number): string {
  const parts: string[] = []
  if (files > 0) {
    parts.push(tString('fileOperations.transferDialog.filesPart', { countText: String(files), count: files }))
  }
  if (folders > 0) {
    parts.push(tString('fileOperations.transferDialog.foldersPart', { countText: String(folders), count: folders }))
  }
  if (parts.length === 0) {
    return tString('fileOperations.transferDialog.titleVerbOnly', { verb: operationType })
  }
  const phrase = parts.length === 2 ? tString('fileOperations.shared.andJoin', { a: parts[0], b: parts[1] }) : parts[0]
  return tString('fileOperations.transferDialog.titleWithCounts', { verb: operationType, phrase })
}

/**
 * Extracts the folder name from a full path.
 * Handles root paths, trailing slashes, and GVFS SMB share directories.
 * @param path - Full path like "/Users/john/Documents"
 * @returns The last path component, like "Documents"
 */
export function getFolderName(path: string): string {
  if (path === '/') return '/'
  const normalized = path.endsWith('/') ? path.slice(0, -1) : path
  const parts = normalized.split('/')
  const last = parts[parts.length - 1] || '/'
  // GVFS SMB share directories: extract just the share name
  const smbMatch = last.match(/^smb-share:.*share=([^,]+)/)
  if (smbMatch) return smbMatch[1]
  return last
}

/**
 * Derives the user-facing label for one side of the transfer direction header.
 *
 * Normally the basename is the right thing to show ("photos" for
 * `/mtp-20-5/65538/photos`). But at a volume root the last path segment isn't a
 * user-meaningful name — for an MTP storage root the basename is the raw storage
 * id (`65538` = 0x10002), which surfaced as "65538 <- cmdr" in the header. When
 * the path IS the volume root (or empty / "/"), fall back to the volume's
 * display name (like "Virtual Pixel 9 - SD Card"). A missing display name falls
 * back to the basename so the label never blanks.
 *
 * @param path - The folder path for this side (source or destination)
 * @param volumeRootPath - The root path of the volume this folder lives on
 * @param volumeDisplayName - The volume's display name from the volume store
 */
export function deriveTransferLabel(path: string, volumeRootPath: string, volumeDisplayName: string): string {
  const normPath = path.endsWith('/') && path !== '/' ? path.slice(0, -1) : path
  const normRoot = volumeRootPath.endsWith('/') && volumeRootPath !== '/' ? volumeRootPath.slice(0, -1) : volumeRootPath
  const atRoot = normPath === '' || normPath === '/' || normPath === normRoot
  if (atRoot && volumeDisplayName !== '') {
    return volumeDisplayName
  }
  return getFolderName(path)
}

/**
 * Converts frontend indices to backend indices.
 *
 * When a directory listing has a parent entry ("..") shown at index 0,
 * the frontend indices are offset by 1 from the backend indices.
 * This function adjusts for that offset and filters out invalid indices.
 *
 * @example
 * // With hasParent=true, frontend [1,2,3] becomes backend [0,1,2]
 * toBackendIndices([1, 2, 3], true) // => [0, 1, 2]
 *
 * // With hasParent=false, indices pass through unchanged
 * toBackendIndices([0, 1, 2], false) // => [0, 1, 2]
 *
 * // Index 0 with hasParent=true is filtered (it's the ".." entry)
 * toBackendIndices([0, 1, 2], true) // => [0, 1]
 */
export function toBackendIndices(frontendIndices: number[], hasParent: boolean): number[] {
  return frontendIndices.map((i) => (hasParent ? i - 1 : i)).filter((i) => i >= 0)
}

/**
 * Converts a frontend cursor index to a backend index.
 *
 * Returns null if the cursor is on the ".." entry (index 0 when hasParent=true)
 * or if the index is invalid.
 *
 * @example
 * toBackendCursorIndex(5, true)  // => 4 (adjusted for ".." entry)
 * toBackendCursorIndex(5, false) // => 5 (no adjustment needed)
 * toBackendCursorIndex(0, true)  // => null (cursor on ".." entry)
 * toBackendCursorIndex(-1, false) // => null (invalid index)
 */
export function toBackendCursorIndex(frontendIndex: number, hasParent: boolean): number | null {
  if (frontendIndex < 0) return null
  if (hasParent && frontendIndex === 0) return null // ".." entry
  return hasParent ? frontendIndex - 1 : frontendIndex
}

/** Strips the volume prefix to get a volume-relative path. Always returns a `/`-prefixed string. */
export function toVolumeRelativePath(fullPath: string, volumePath: string): string {
  // MTP/non-local volumes: fullPath may already be volume-relative (like "/DCIM")
  // while volumePath is a URL (like "mtp://device/storage"). Just pass through.
  if (!fullPath.startsWith(volumePath) && volumePath.includes('://')) {
    return fullPath || '/'
  }
  if (volumePath === '/') return fullPath
  if (fullPath.startsWith(volumePath)) {
    return fullPath.slice(volumePath.length) || '/'
  }
  return '/'
}

/**
 * Whether to show the "X will be written, source is Y" hardlink note in the
 * transfer dialog. A copy materializes every hardlink as a full independent
 * file, so the bytes written (`writeBytes`, the write footprint) exceed the
 * source's on-disk size (`dedupBytes`, the `du`-equivalent). We surface the
 * gap so the headline size doesn't look wrong against Finder's number.
 *
 * Copy-only: a same-filesystem move renames in place and writes nothing, and
 * the dialog can't know source/dest filesystem-sameness upfront — so we never
 * show a potentially-wrong note for a move. Gated on a completed scan with a
 * real gap (`0 < dedupBytes < writeBytes`); equal values mean no hardlinks.
 */
export function shouldShowHardlinkNote(args: {
  operationType: TransferOperationType
  scanComplete: boolean
  writeBytes: number
  dedupBytes: number
}): boolean {
  const { operationType, scanComplete, writeBytes, dedupBytes } = args
  return operationType === 'copy' && scanComplete && dedupBytes > 0 && dedupBytes < writeBytes
}

/** The i18n key for the transfer dialog's primary confirm button, per mode. */
export function confirmLabelKey(operationType: TransferOperationType): MessageKey {
  if (operationType === 'copy') return 'fileOperations.transferDialog.confirmCopy'
  if (operationType === 'compress') return 'fileOperations.transferDialog.confirmCompress'
  return 'fileOperations.transferDialog.confirmMove'
}

/**
 * The dialog's initial volume-relative destination path. For copy/move it's the
 * other pane's folder; for compress it's that folder plus a suggested `.zip`
 * filename (the field stays editable). Keeping the join here lets the dialog set
 * `editedPath` in one line and unit-test the compress default via
 * `suggestCompressArchiveName`.
 */
export function initialEditedPath(
  operationType: TransferOperationType,
  destinationPath: string,
  volumePath: string,
  sourcePaths: string[],
  sourceFolderPath: string,
): string {
  const folder = toVolumeRelativePath(destinationPath, volumePath)
  if (operationType !== 'compress') return folder
  const name = suggestCompressArchiveName(sourcePaths, sourceFolderPath)
  const base = folder === '/' ? '' : folder.replace(/\/+$/, '')
  return `${base}/${name}`
}
