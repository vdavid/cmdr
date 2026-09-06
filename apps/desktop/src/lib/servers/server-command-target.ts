/**
 * Which server the palette's server commands act on.
 *
 * ❗ **The hub IS a pane**, so a command reading "the focused pane's volume" would
 * answer the synthetic hub row rather than the server the cursor is on. The rule
 * is: the hub's cursor row wherever there is one, the pane's own volume
 * otherwise, and nothing at all when neither names a place the servers family
 * speaks for.
 *
 * Pure, so the rule is one table of cells rather than a thing to reason about at
 * three call sites.
 */

import { isServerPlaceRow } from '$lib/file-explorer/navigation/server-row-actions'
import type { HubRow } from '$lib/file-explorer/network/servers-hub-rows'
import type { VolumeInfo } from '$lib/file-explorer/types'

/** The place a server command will act on. */
export interface ServerCommandTarget {
  volumeId: string
  /** What to call it in a confirmation or a toast. */
  name: string
  /**
   * Whether it is pinned to the switcher, or `null` when this reading can't say.
   *
   * A `VolumeInfo` carries no pin, so a target resolved from the pane's volume
   * answers `null` and the caller reads the saved list before flipping it.
   */
  pinned: boolean | null
}

/** What the resolver reads. */
export interface ServerCommandSources {
  /** The hub row under the focused pane's cursor, when that pane is the hub. */
  cursorRow: HubRow | null
  /** The focused pane's own volume, from the volume list. */
  paneVolume: VolumeInfo | null
}

/** The place to act on, or `null` when nothing in view is one. */
export function serverCommandTarget(sources: ServerCommandSources): ServerCommandTarget | null {
  const row = sources.cursorRow
  if (row?.volumeId) {
    return { volumeId: row.volumeId, name: row.name, pinned: row.pinned }
  }
  // ❗ A row with no `volumeId` is an SMB host, whose places are mounted shares
  // with ids `statfs` mints. It stops the search rather than falling through to
  // the pane's volume: the user is pointing at that host, and acting on
  // something else instead is worse than doing nothing.
  if (row) return null

  const volume = sources.paneVolume
  if (!volume || !isServerPlaceRow(volume)) return null
  return { volumeId: volume.id, name: volume.name, pinned: null }
}
