import type { FilePaneAPI } from '#lib/file-explorer/pane/types.js'

/** What the pre-fill helpers need to know about the entry under the cursor. */
export interface CursorEntry {
  name: string
  isDirectory: boolean
}

/**
 * The entry under the pane's cursor, or `null` when there is none: no pane, the
 * cursor on the `..` row, rows that never settled, or a lookup that fails. Read
 * through the pane's checked cursor read (`refreshCursorEntry`), so a watcher
 * insert can't pre-fill the neighbor's name; quiet on a refusal, since an empty
 * pre-fill already says it. Shared by the New folder and New file pre-fills so
 * the two can't drift.
 */
export async function getCursorEntry(paneRef: FilePaneAPI | undefined): Promise<CursorEntry | null> {
  try {
    const entry = await paneRef?.refreshCursorEntry({ announce: false })
    return entry && entry.name !== '..' ? entry : null
  } catch {
    return null
  }
}
