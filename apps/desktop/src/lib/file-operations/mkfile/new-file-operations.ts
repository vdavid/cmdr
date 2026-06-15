import type { FilePaneAPI } from '$lib/file-explorer/pane/types'

/**
 * Returns the full filename (with extension) for the entry under cursor, or empty string
 * if the cursor is on a directory or ".." entry.
 */
export async function getInitialFileName(
  paneRef: FilePaneAPI | undefined,
  paneListingId: string,
  showHiddenFiles: boolean,
  getFileAt: (
    listingId: string,
    index: number,
    showHiddenFiles: boolean,
  ) => Promise<{ name: string; isDirectory: boolean } | null>,
): Promise<string> {
  try {
    const cursorIndex = paneRef?.getCursorIndex()
    const hasParent = paneRef?.hasParentEntry()
    if (cursorIndex === undefined || cursorIndex < 0) return ''
    const backendIndex = hasParent ? cursorIndex - 1 : cursorIndex
    if (backendIndex < 0) return ''
    const entry = await getFileAt(paneListingId, backendIndex, showHiddenFiles)
    if (!entry) return ''
    // Files: return full name with extension. Directories: return empty (not useful as a file name hint).
    return entry.isDirectory ? '' : entry.name
  } catch {
    return ''
  }
}
