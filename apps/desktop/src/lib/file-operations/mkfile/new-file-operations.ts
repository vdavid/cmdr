import type { FilePaneAPI } from '$lib/file-explorer/pane/types'
import { getCursorEntry, type GetFileAtFn } from '../cursor-entry'

/**
 * The New file pre-fill: the cursor entry's full filename, extension kept.
 * Empty for a directory (no use as a file name hint) and for `..`.
 */
export async function getInitialFileName(
  paneRef: FilePaneAPI | undefined,
  paneListingId: string,
  showHiddenFiles: boolean,
  getFileAt: GetFileAtFn,
): Promise<string> {
  const entry = await getCursorEntry(paneRef, paneListingId, showHiddenFiles, getFileAt)
  if (!entry) return ''
  return entry.isDirectory ? '' : entry.name
}
