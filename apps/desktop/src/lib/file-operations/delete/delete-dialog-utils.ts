/**
 * Utility functions for the delete confirmation dialog.
 * Pure functions — no Svelte reactivity, no side effects.
 */

/** Minimal item info needed by DeleteDialog for display. */
export interface DeleteSourceItem {
  name: string
  size?: number
  isDirectory: boolean
  isSymlink: boolean
  recursiveSize?: number
  recursiveFileCount?: number
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
    if (folderCount > 0) return 'Delete 1 folder under cursor'
    return 'Delete 1 file under cursor'
  }

  const parts: string[] = []
  if (fileCount > 0) {
    parts.push(`${String(fileCount)} selected ${fileCount === 1 ? 'file' : 'files'}`)
  }
  if (folderCount > 0) {
    parts.push(`${String(folderCount)} ${folderCount === 1 ? 'folder' : 'folders'}`)
  }
  if (parts.length === 0) return 'Delete'
  return `Delete ${parts.join(' and ')}`
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
    return 'This item is a symlink. Only the link will be deleted, not its target.'
  }
  return `Your selection includes ${String(count)} ${count === 1 ? 'symlink' : 'symlinks'}. Only the links themselves will be deleted, not their targets.`
}

/** Max items to show in the scrollable file list before overflow. */
export const MAX_VISIBLE_ITEMS = 10
