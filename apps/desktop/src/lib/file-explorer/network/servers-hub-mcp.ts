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

import type { PaneFileEntry, PaneState } from '$lib/tauri-commands'
import { parseServerPath, serverAppRoot } from '$lib/servers/server-path-utils'
import type { HubRow } from './servers-hub-rows'
import { fullIndexOf, type HubItem } from './servers-hub-items'

/**
 * The nearby group's header, as an agent sees it: English whatever the locale,
 * like the status tokens. `move_cursor` finds it by this name, and
 * `open_under_cursor` on it opens or collapses the group.
 */
export const NEARBY_GROUP_MCP_NAME = 'Found nearby'

/** Where the group's header points. A sentinel: it leads nowhere. */
export const NEARBY_GROUP_MCP_PATH = 'smb://nearby'

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

/**
 * The hub as the pane state MCP mirrors: every item, then the add row, and the cursor.
 *
 * ❗ `items` is the FULL list and `visibleCursorIndex` counts what is on screen:
 * the servers a collapsed group hides are still listed (an agent asking which
 * servers exist gets the truth, and the group's `state=` says they're folded
 * away), so the cursor is re-counted over the list the agent indexes into.
 */
export function hubPaneState(
  items: HubItem[],
  visibleCursorIndex: number,
  volumeName: string,
  lookups: HubMcpLookups,
): PaneState {
  return {
    path: 'smb://',
    volumeId: 'network',
    volumeName,
    files: hubMcpEntries(items, lookups),
    cursorIndex: fullIndexOf(items, visibleCursorIndex),
    viewMode: 'full',
    selectedIndices: [],
    totalFiles: items.length,
    loadedStart: 0,
    loadedEnd: items.length,
  }
}

/** One entry per item, then the add row. */
export function hubMcpEntries(items: HubItem[], lookups: HubMcpLookups): PaneFileEntry[] {
  const entries = items.map((item) => (item.kind === 'row' ? entryFor(item.row, lookups) : groupEntry(item)))
  entries.push(emptyEntry(ADD_SERVER_MCP_NAME, ADD_SERVER_MCP_PATH, false))
  return entries
}

/** The nearby group's header: whether it's open, and how many servers it holds. */
function groupEntry(group: Extract<HubItem, { kind: 'nearby_group' }>): PaneFileEntry {
  const tokens = ['kind=group', `state=${group.expanded ? 'expanded' : 'collapsed'}`, `servers=${String(group.count)}`]
  return emptyEntry(`${NEARBY_GROUP_MCP_NAME}  ${tokens.join('  ')}`, NEARBY_GROUP_MCP_PATH, false)
}

function entryFor(row: HubRow, lookups: HubMcpLookups): PaneFileEntry {
  const tokens = [`protocol=${row.protocol}`, `status="${row.status}"`, `address=${row.address}`]
  // The account it's signed in as (a share: opens as); `(guest)` can't be an account's name.
  if (row.account !== null) tokens.push(`account=${row.account.kind === 'guest' ? '(guest)' : row.account.username}`)
  if (row.kind === 'place') {
    // A saved place under the row above it: an SMB share, or an S3 bucket or account root.
    tokens.push(row.protocol === 'smb' ? 'kind=share' : 'kind=place')
  } else {
    const shares = lookups.shareCountOf?.(row)
    if (shares !== undefined) tokens.push(`shares=${String(shares)}`)
  }
  return emptyEntry(`${row.name}  ${tokens.join('  ')}`, pathFor(row, lookups), true)
}

/**
 * Where the row leads.
 *
 * An SMB host keeps the `smb://<address>` spelling the host list has always
 * published; a one-place row and a saved share publish their app root, which
 * `nav_to_path` can actually take.
 */
function pathFor(row: HubRow, lookups: HubMcpLookups): string {
  // A share's place: its last mount path, or `smb://<host>/<share>` before one.
  if (row.kind === 'place' && row.place) return row.place.appRoot
  if (row.protocol === 'smb') return `smb://${row.address}`
  // An S3 account: the account's own prefix, which every one of its places hangs under.
  if (row.protocol === 's3') {
    const parsed = parseServerPath(row.saved?.places[0]?.appRoot ?? '')
    if (parsed) return serverAppRoot(parsed)
  }
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
    recursiveSizeUpdating: null,
  }
}
