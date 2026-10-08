/**
 * The hub's list as the cursor walks it: the saved servers, then ONE header for
 * the servers Cmdr only found nearby, then those servers.
 *
 * Pure, like `servers-hub-rows.ts`, which it sits on top of: the rows say what
 * exists, this says how it's grouped and what is on screen.
 *
 * ❗ **Two index spaces, and the hidden rows are why.** The cursor counts what is
 * ON SCREEN (`visibleHubItems`), so arrows, Page Up, and Home never land on a row
 * a collapsed group is hiding. An agent counts the FULL list (`hubItems`), which
 * keeps the nearby servers whatever the group's state, because "which servers
 * are there?" deserves the truth. `fullIndexOf` and `visibleIndexOf` cross
 * between the two, and both put "Add server…" one past the last item.
 */

import type { NearbyServersGroupChoice } from '$lib/settings'
import { isNearbyOnly, type HubRow } from './servers-hub-rows'

/** The header's id. Row ids are server and host ids, or `share:<volume id>`, so it can't meet one. */
export const NEARBY_GROUP_ID = 'group:nearby'

/** One line the cursor can sit on, short of "Add server…". */
export type HubItem =
  | { kind: 'row'; id: string; row: HubRow; inNearbyGroup: boolean }
  | { kind: 'nearby_group'; id: typeof NEARBY_GROUP_ID; count: number; expanded: boolean }

/**
 * Whether the nearby group is open: what the person last chose, else collapsed
 * for someone with a saved server and expanded for someone with none, for whom
 * the nearby servers are the whole view.
 */
export function isNearbyGroupExpanded(choice: NearbyServersGroupChoice, hasSavedServer: boolean): boolean {
  if (choice !== 'auto') return choice === 'expanded'
  return !hasSavedServer
}

/**
 * Every item, hidden ones included. The header is there only while something was
 * found nearby.
 *
 * ❗ Counts on the rows' order: `buildHubRows` puts the found-only hosts last.
 */
export function hubItems(rows: HubRow[], nearbyExpanded: boolean): HubItem[] {
  const nearbyStart = rows.findIndex(isNearbyOnly)
  const asItem = (row: HubRow, at: number): HubItem => ({
    kind: 'row',
    id: row.id,
    row,
    inNearbyGroup: nearbyStart >= 0 && at >= nearbyStart,
  })
  const items = rows.map(asItem)
  if (nearbyStart < 0) return items
  const count = rows.length - nearbyStart
  items.splice(nearbyStart, 0, { kind: 'nearby_group', id: NEARBY_GROUP_ID, count, expanded: nearbyExpanded })
  return items
}

/** What is on screen: everything but the rows a collapsed group hides. */
export function visibleHubItems(items: HubItem[]): HubItem[] {
  return items.filter((item) => !isHidden(items, item))
}

/** Where the item at `visibleIndex` sits in the full list. */
export function fullIndexOf(items: HubItem[], visibleIndex: number): number {
  const item = visibleHubItems(items).at(visibleIndex)
  return item && visibleIndex >= 0 ? items.indexOf(item) : items.length
}

/** Where the item at `fullIndex` sits on screen, or `null` when a collapsed group hides it. */
export function visibleIndexOf(items: HubItem[], fullIndex: number): number | null {
  const visible = visibleHubItems(items)
  if (fullIndex >= items.length) return visible.length
  const at = visible.indexOf(items[fullIndex])
  return at >= 0 ? at : null
}

function isNearbyRow(item: HubItem): boolean {
  return item.kind === 'row' && item.inNearbyGroup
}

function isHidden(items: HubItem[], item: HubItem): boolean {
  return isNearbyRow(item) && items.some((other) => other.kind === 'nearby_group' && !other.expanded)
}
