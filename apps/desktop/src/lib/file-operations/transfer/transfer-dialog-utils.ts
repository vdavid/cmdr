/**
 * Utility functions for the transfer (copy/move) dialog. User-facing copy
 * resolves through the i18n catalog (`fileOperations.transferDialog.*`).
 */

import type { TransferOperationType } from '$lib/file-explorer/types'
import type { MessageKey } from '$lib/intl/keys.gen'
import { tString } from '$lib/intl/messages.svelte'
import { isPathOnVolume, isPlainFilesystemPath, parentOf, toCanonical } from '$lib/path/canonical'
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
 * The folder a file sits in, for a dialog that has named the file and now has
 * to say WHICH one it means. A conflict prompt showing a bare `f001` is
 * ambiguous the moment two folders hold that name, and a copy of a deep tree is
 * exactly where that happens.
 *
 * `null` when the path isn't one we can safely take a parent of (relative, or
 * `~`-rooted). Backend paths are absolute or virtual-volume URLs, so that is a
 * bug elsewhere rather than something a dialog should render an error about:
 * the caller falls back to showing the name alone.
 */
export function containingFolder(path: string): string | null {
  try {
    return parentOf(toCanonical(path, ''))
  } catch {
    return null
  }
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

/**
 * Strips the volume prefix to get a volume-relative path. Always returns a
 * `/`-prefixed string, which is what the destination box's absolute-path check
 * demands.
 *
 * ❗ **Trim the volume root's trailing slash before slicing.** A remote volume is
 * rooted at `<prefix><server-side root>`, so one rooted at `/` (the DEFAULT for
 * SFTP, WebDAV, and ADB) really does spell itself `sftp://ada@nas.local:22/`.
 * Slicing by raw length then eats the separator and yields `home/ada/…`, which
 * `validateDirectoryPath` rejects and `handleConfirm` refuses to act on, so the
 * Copy button does nothing at all. A volume rooted at a subfolder has no trailing
 * slash and never showed the bug, which is how it read as intermittent.
 *
 * Membership is `isPathOnVolume`, i.e. by whole components, so a sibling root can't
 * borrow this one's prefix and hand back `-1/photos`.
 */
export function toVolumeRelativePath(fullPath: string, volumePath: string): string {
  if (volumePath === '/') return fullPath
  const root = volumePath.endsWith('/') ? volumePath.slice(0, -1) : volumePath
  if (isPathOnVolume(fullPath, root)) return fullPath.slice(root.length) || '/'
  // A non-local volume whose caller already spelled the path volume-relative
  // ("/DCIM" against "mtp://device/storage"). Another volume's URL is not ours,
  // and echoing it into the box would just fail the absolute-path check.
  if (root.includes('://') && fullPath.startsWith('/') && !fullPath.includes('://')) return fullPath
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

/** Copy and Move name a single destination item; batches target a folder. */
export function initialEditedPath(
  operationType: TransferOperationType,
  destinationPath: string,
  volumePath: string,
  sourcePaths: string[],
  sourceFolderPath: string,
): string {
  const singleTransfer = (operationType === 'copy' || operationType === 'move') && sourcePaths.length === 1
  const folder =
    singleTransfer && isPlainFilesystemPath(volumePath)
      ? destinationPath
      : toVolumeRelativePath(destinationPath, volumePath)
  if (operationType !== 'compress' && !singleTransfer) return folder
  const name = singleTransfer
    ? getFolderName(sourcePaths[0])
    : suggestCompressArchiveName(sourcePaths, sourceFolderPath)
  const base = folder === '/' ? '' : folder.replace(/\/+$/, '')
  return `${base}/${name}`
}

/** `folder` + `/` + `leaf`, with no doubled slash at the root or after a trailing one. */
export function joinPathLeaf(folder: string, leaf: string): string {
  const base = folder.replace(/\/+$/, '')
  return `${base}/${leaf}`
}

/**
 * The rename-mode path box split back into the folder the source moves into and
 * the name it lands under. Everything after the last slash is the name, so a
 * trailing slash (the user deleted the name) gives an empty one for validation
 * to refuse, ❌ never the folder's own name one level up.
 */
export function splitPathLeaf(path: string): { folder: string; leaf: string } {
  const slash = path.lastIndexOf('/')
  if (slash === -1) return { folder: '/', leaf: path }
  const folder = path.slice(0, slash).replace(/\/+$/, '')
  return { folder: folder === '' ? '/' : folder, leaf: path.slice(slash + 1) }
}

/**
 * Split a complete transfer target, resolving relative paths against the source folder.
 * A trailing slash means "into this folder": the item keeps `intoFolderName` (its own name),
 * so a pasted folder path never turns into the item's new name.
 */
export function resolveTransferFilename(
  path: string,
  sourceFolder: string,
  homePath = '',
  intoFolderName?: string,
): { parent: string; name: string } | null {
  const trimmed = path.trim()
  const entered = trimmed.endsWith('/') && intoFolderName ? `${trimmed}${intoFolderName}` : trimmed
  const leaf = entered.split('/').at(-1)
  if (!leaf || leaf === '.' || leaf === '..' || entered === '~') return null
  const expanded = entered.startsWith('~/') && homePath ? `${homePath}/${entered.slice(2)}` : entered
  const absolute = expanded.startsWith('/') ? expanded : `${sourceFolder}/${expanded}`
  // Dot segments cannot obscure a destination inside the source subtree.
  const parts: string[] = []
  for (const part of absolute.split('/')) {
    if (!part || part === '.') continue
    if (part === '..') parts.pop()
    else parts.push(part)
  }
  const name = parts.pop()
  return name ? { parent: `/${parts.join('/')}`, name } : null
}

/** Keep the target folder when switching between folder and filename modes. */
export function editedPathAfterOperationChange(args: {
  operationType: TransferOperationType
  nextOperationType: TransferOperationType
  editedPath: string
  targetParent?: string
  volumePath: string
  sourcePaths: string[]
  sourceFolderPath: string
}): string {
  const { operationType, nextOperationType, editedPath, targetParent, volumePath, sourcePaths, sourceFolderPath } = args
  if (sourcePaths.length === 1 && operationType !== 'compress' && nextOperationType !== 'compress') return editedPath
  let folder =
    targetParent ?? (operationType === 'compress' ? (containingFolder(editedPath) ?? editedPath) : editedPath)
  if (volumePath !== '/' && isPlainFilesystemPath(volumePath) && !isPathOnVolume(folder, volumePath)) {
    folder = `${volumePath.replace(/\/+$/, '')}/${folder.replace(/^\/+/, '')}`
  }
  return initialEditedPath(nextOperationType, folder, volumePath, sourcePaths, sourceFolderPath)
}
