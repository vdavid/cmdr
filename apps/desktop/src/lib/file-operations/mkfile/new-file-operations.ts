import type { FilePaneAPI } from '#lib/file-explorer/pane/types.js'
import { getCursorEntry } from '../cursor-entry'

/**
 * The New file pre-fill: the cursor entry's full filename, extension kept.
 * Empty for a directory (no use as a file name hint) and for `..`.
 */
export async function getInitialFileName(paneRef: FilePaneAPI | undefined): Promise<string> {
  const entry = await getCursorEntry(paneRef)
  if (!entry) return ''
  return entry.isDirectory ? '' : entry.name
}
