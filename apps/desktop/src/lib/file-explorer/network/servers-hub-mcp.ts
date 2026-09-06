/**
 * The hub, as an agent reading `cmdr://state` sees it.
 *
 * MCP's `PaneFileEntry` has only `name`, `path`, and `isDirectory`, so the
 * columns a person reads are encoded into the name as `key=value` tokens. That
 * is the same trick the host list has always used; what changed is which
 * columns the hub shows.
 *
 * ❗ **The status token is locale-independent**, deliberately, even though the
 * column beside it is translated: `smb.spec.ts` polls on these strings and an
 * agent parses them, so a translation landing in the wire would break both
 * silently. The words a person reads come from `servers.hub.status.*`.
 */

import type { PaneFileEntry } from '$lib/tauri-commands'
import type { HubRow } from './servers-hub-rows'

/** The add row's name, as an agent sees it. */
export const ADD_SERVER_MCP_NAME = '+ Add server…'

/** Where the add row points. A sentinel, like the hub's own `smb://`. */
export const ADD_SERVER_MCP_PATH = 'smb://add'

/** What the encoder has to ask the app about a row. */
export interface HubMcpLookups {
  /**
   * The app root a one-place row addresses its files by, when the app knows it.
   *
   * Comes from the saved place (`SavedPlace.appRoot`), which Rust mints in one
   * function; ❌ never re-spelled here, because a prefix that folds differently
   * misses the volume its own id names.
   */
  appRootOf: (row: HubRow) => string | null
  /** How many shares an SMB host has, when a listing answered. */
  shareCountOf?: (row: HubRow) => number | undefined
}

/** One entry per row, then the add row. */
export function hubMcpEntries(rows: HubRow[], lookups: HubMcpLookups): PaneFileEntry[] {
  const entries = rows.map((row) => entryFor(row, lookups))
  entries.push(emptyEntry(ADD_SERVER_MCP_NAME, ADD_SERVER_MCP_PATH, false))
  return entries
}

function entryFor(row: HubRow, lookups: HubMcpLookups): PaneFileEntry {
  const tokens = [`protocol=${row.protocol}`, `status="${row.status}"`, `address=${row.address}`]
  const shares = lookups.shareCountOf?.(row)
  if (shares !== undefined) tokens.push(`shares=${String(shares)}`)
  return emptyEntry(`${row.name}  ${tokens.join('  ')}`, pathFor(row, lookups), true)
}

/**
 * Where the row leads.
 *
 * An SMB host keeps the `smb://<address>` spelling the host list has always
 * published; a one-place row publishes its app root, which `nav_to_path` can
 * actually take.
 */
function pathFor(row: HubRow, lookups: HubMcpLookups): string {
  if (row.protocol === 'smb') return `smb://${row.address}`
  return lookups.appRootOf(row) ?? `${row.protocol}://${row.address}`
}

/** A `PaneFileEntry` with every size and date slot empty, which every hub row is. */
function emptyEntry(name: string, path: string, isDirectory: boolean): PaneFileEntry {
  return {
    name,
    path,
    isDirectory,
    size: null,
    recursiveSize: null,
    modified: null,
    recursiveSizePending: null,
  }
}
