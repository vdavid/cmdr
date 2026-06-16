/**
 * Utility functions for the delete confirmation dialog.
 * No Svelte reactivity, no side effects. User-facing copy resolves through the
 * i18n catalog (`fileOperations.delete.*`) via `tString`; called in plain `.ts`
 * these are snapshots at call time, which is right for a transient dialog.
 */

import { tString } from '$lib/intl/messages.svelte'

/** Minimal item info needed by DeleteDialog for display.
 *  Group A wire-format: IPC sends `null` for absent FileEntry fields, not `undefined`.
 *  Accept both so the constructed item shape matches whatever the caller passes through. */
export interface DeleteSourceItem {
  name: string
  size?: number | null
  isDirectory: boolean
  isSymlink: boolean
  recursiveSize?: number | null
  recursiveFileCount?: number | null
}

/**
 * Generates the dialog title based on the selection source and item counts.
 *
 * Selected items:  "Delete 3 selected files and 1 folder"
 * Cursor item:     "Delete 1 file under cursor" / "Delete 1 folder under cursor"
 */
export function generateDeleteTitle(items: DeleteSourceItem[], isFromCursor: boolean): string {
  const fileCount = items.filter((i) => !i.isDirectory).length
  const folderCount = items.filter((i) => i.isDirectory).length

  if (isFromCursor) {
    if (folderCount > 0) return tString('fileOperations.delete.titleCursorFolder')
    return tString('fileOperations.delete.titleCursorFile')
  }

  const parts: string[] = []
  if (fileCount > 0) {
    parts.push(tString('fileOperations.delete.selectedFilesPart', { countText: String(fileCount), count: fileCount }))
  }
  if (folderCount > 0) {
    parts.push(tString('fileOperations.delete.foldersPart', { countText: String(folderCount), count: folderCount }))
  }
  if (parts.length === 0) return tString('fileOperations.delete.titleFallback')
  const phrase = parts.length === 2 ? tString('fileOperations.shared.andJoin', { a: parts[0], b: parts[1] }) : parts[0]
  return tString('fileOperations.delete.titleSelected', { phrase })
}

/**
 * Abbreviates a path by replacing the user's home directory with ~.
 * Uses the /Users/<username> pattern (macOS convention).
 */
export function abbreviatePath(path: string): string {
  const userMatch = path.match(/^(\/Users\/[^/]+)(.*)$/)
  if (userMatch) {
    return '~' + userMatch[2]
  }
  return path
}

/** Counts how many items in the list are symlinks. */
export function countSymlinks(items: DeleteSourceItem[]): number {
  return items.filter((i) => i.isSymlink).length
}

/**
 * Generates the symlink notice text, or null if there are no symlinks.
 *
 * - Single symlink: "This item is a symlink. Only the link will be deleted, not its target."
 * - Multiple: "Your selection includes N symlinks. Only the links themselves will be deleted, not their targets."
 */
export function getSymlinkNotice(items: DeleteSourceItem[]): string | null {
  const count = countSymlinks(items)
  if (count === 0) return null
  if (count === 1 && items.length === 1) {
    return tString('fileOperations.delete.symlinkNoticeSingle')
  }
  return tString('fileOperations.delete.symlinkNoticeMany', { countText: String(count), count })
}

/** Max items to show in the scrollable file list before overflow. */
export const MAX_VISIBLE_ITEMS = 10
