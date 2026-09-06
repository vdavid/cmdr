/**
 * The MCP mirror of a search-results snapshot pane's rows.
 *
 * A snapshot pane has no backend listing, so `pane-mcp-sync`'s ordinary
 * `getFileRange(listingId, …)` path has nothing to ask. The rows are already in
 * hand (the frontend snapshot store owns them), so this maps the visible window
 * of them straight onto the wire shape.
 *
 * The Name column of that pane shows a friendly FULL PATH, but MCP reports the
 * BASENAME in `name` and the absolute path in `path`, exactly like every other
 * pane: `move_cursor`'s name lookup, `select { names: [...] }`, and the pane
 * view's own `findItemIndex` all match on basenames.
 */

import type { SearchResultEntry } from '$lib/ipc/bindings'
import type { PaneFileEntry } from '$lib/tauri-commands'

/**
 * The snapshot's rows over `[start, end)`, capped at `maxRows`.
 *
 * A snapshot pane renders no `..` row, so frontend indices and snapshot indices
 * are the same number and the window needs no parent offset. Every recursive
 * field stays `null`: a search result carries per-file basics only, and claiming
 * a folder total nobody computed would be worse than saying nothing.
 */
export function snapshotMcpRows(
  entries: readonly SearchResultEntry[],
  start: number,
  end: number,
  maxRows: number,
): PaneFileEntry[] {
  const from = Math.max(0, start)
  const to = Math.min(entries.length, Math.max(from, end), from + maxRows)
  const rows: PaneFileEntry[] = []
  for (let i = from; i < to; i++) {
    const entry = entries[i]
    rows.push({
      name: entry.name,
      path: entry.path,
      isDirectory: entry.isDirectory,
      size: entry.size ?? null,
      recursiveSize: null,
      modified: entry.modifiedAt != null ? new Date(entry.modifiedAt * 1000).toISOString() : null,
      recursiveSizePending: null,
      recursiveSizeComplete: null,
      recursiveSizeStale: null,
      recursivePhysicalSize: null,
      tags: [],
    })
  }
  return rows
}
