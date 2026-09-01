import type { FilePaneAPI } from '$lib/file-explorer/pane/types'

/** What the pre-fill helpers need to know about the entry under the cursor. */
export interface CursorEntry {
  name: string
  isDirectory: boolean
}

/**
 * The `getFileAt` IPC command's shape, taken as a parameter so the pre-fill
 * helpers stay testable with a stub.
 */
export type GetFileAtFn = (listingId: string, index: number, showHiddenFiles: boolean) => Promise<CursorEntry | null>

/**
 * The backend entry under the pane's cursor, or `null` when there is none: no
 * pane, the cursor on the `..` row, or a lookup that fails. The pane's cursor
 * index counts the `..` row and the backend listing doesn't, hence the shift.
 * Shared by the New folder and New file pre-fills so the two can't drift.
 */
export async function getCursorEntry(
  paneRef: FilePaneAPI | undefined,
  paneListingId: string,
  showHiddenFiles: boolean,
  getFileAt: GetFileAtFn,
): Promise<CursorEntry | null> {
  try {
    const cursorIndex = paneRef?.getCursorIndex()
    const hasParent = paneRef?.hasParentEntry()
    if (cursorIndex === undefined || cursorIndex < 0) return null
    const backendIndex = hasParent ? cursorIndex - 1 : cursorIndex
    if (backendIndex < 0) return null
    return await getFileAt(paneListingId, backendIndex, showHiddenFiles)
  } catch {
    return null
  }
}
