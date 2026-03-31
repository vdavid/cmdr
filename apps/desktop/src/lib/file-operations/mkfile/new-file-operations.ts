import type FilePane from '$lib/file-explorer/pane/FilePane.svelte'

/**
 * Returns the full filename (with extension) for the entry under cursor, or empty string
 * if the cursor is on a directory or ".." entry.
 */
export async function getInitialFileName(
  paneRef: FilePane | undefined,
  paneListingId: string,
  showHiddenFiles: boolean,
  getFileAt: (
    listingId: string,
    index: number,
    showHiddenFiles: boolean,
  ) => Promise<{ name: string; isDirectory: boolean } | null>,
): Promise<string> {
  try {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const cursorIndex = paneRef?.getCursorIndex?.() as number | undefined
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const hasParent = paneRef?.hasParentEntry?.() as boolean | undefined
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
