import type { HubItem } from './servers-hub-items'

/**
 * Where the cursor goes when the list on screen is rebuilt: onto the SAME item
 * (by id) wherever it moved, the "Add server…" row stays the add row, and an item
 * that left clamps to what's there. ❗ An index alone lands on a neighbour whenever
 * the list re-sorts under it, which is what a plain Add did to its fresh selection.
 *
 * Both lists are the VISIBLE items (`visibleHubItems`), so a group that collapses
 * over the cursor is an item that left: the cursor goes to the group's header,
 * ❌ never to a row nobody can see, and not to whatever index the clamp lands on.
 */
export function cursorAcrossRebuild(before: HubItem[], after: HubItem[], cursor: number): number {
  // Nothing listed yet: the cursor rests on the first server once there is one,
  // ❌ not on "Add server…", which is all an empty list has, and not on the
  // header of an open group when the servers under it are the whole list.
  if (before.length === 0) {
    const first = after.length > 1 && after[0].kind === 'nearby_group' && after[0].expanded ? 1 : 0
    return Math.min(Math.max(cursor, first), after.length)
  }
  if (cursor >= before.length) return after.length
  const was = before[cursor]
  const at = after.findIndex((item) => item.id === was.id)
  if (at >= 0) return at
  const header = after.findIndex((item) => item.kind === 'nearby_group')
  if (was.kind === 'row' && was.inNearbyGroup && header >= 0) return header
  return Math.min(cursor, after.length)
}
