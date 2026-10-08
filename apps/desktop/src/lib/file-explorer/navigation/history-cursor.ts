/**
 * Cursor memory per history entry: the pane reports where its cursor sits, the
 * entry on screen remembers it, and Back / Forward hand the destination's memory
 * back to the pane to restore once its rows land.
 *
 * State belongs to the ENTRY, never to a path: two visits to one folder are two
 * history positions, each with its own cursor.
 */

import type { HistoryCursor, HistoryEntry } from './navigation-history'

/** A cursor position the pane read, tagged with the listing it read it from. */
export interface CursorReading {
  volumeId: string
  path: string
  index: number
  /** Present only when a row read confirmed which row sits at `index`. */
  rowPath?: string
}

/**
 * Writes `reading` into `entry` when it came from that entry's listing.
 *
 * Mutates the entry in place, on purpose: it fires on every cursor move, and a new
 * history object per keystroke would re-run everything that reads the history.
 * The cursor isn't part of the entry's identity, so nothing keyed on it moves.
 *
 * An index reading drops the row once the index moves (the old row no longer
 * describes it). A row reading lands only on the index it was read at, so a
 * slow read for a row the cursor already left can't attach the wrong identity.
 */
export function recordCursor(entry: HistoryEntry, reading: CursorReading): void {
  if (entry.volumeId !== reading.volumeId || entry.path !== reading.path) return
  const { index, rowPath } = reading
  if (rowPath === undefined) {
    if (entry.cursor?.index !== index) entry.cursor = { index }
    return
  }
  if (entry.cursor === undefined || entry.cursor.index === index) entry.cursor = { index, rowPath }
}

/**
 * Where a restored cursor lands among `rowCount` rows: the remembered row where
 * it is now (`foundRowIndex`), else the remembered index clamped into the rows,
 * else the top.
 */
export function restoredCursorIndex(
  cursor: HistoryCursor | undefined,
  foundRowIndex: number | undefined,
  rowCount: number,
): number {
  if (cursor === undefined || rowCount <= 0) return 0
  if (foundRowIndex !== undefined && foundRowIndex >= 0 && foundRowIndex < rowCount) return foundRowIndex
  return Math.max(0, Math.min(cursor.index, rowCount - 1))
}
