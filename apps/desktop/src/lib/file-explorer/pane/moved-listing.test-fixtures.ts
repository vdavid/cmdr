/**
 * A backend listing a watcher insert moved on from what the pane applied, for
 * the specs pinning that delete, the clipboard, and drag refuse old row numbers.
 *
 * The pane applied revision 0, `[alpha, charlie]`, and selected `charlie` on
 * backend row 1. Then `bravo` landed above it: the backend is at revision 1,
 * `[alpha, bravo, charlie]`, so row 1 is now `bravo`, a file nobody selected.
 *
 * It answers the revision-checked reads (`getSelectionSnapshot` /
 * `getSelectionEntries`) the way the backend does: revision 0 is refused as
 * `changed`, so a caller that resolves the press against what the pane applied
 * acts on nothing rather than on `bravo`.
 *
 * Imports only types and the typed refusal, so a spec's `vi.mock` factory can
 * reach it through a lazy `await import`.
 */
import { ListingLookupFailure } from '#lib/tauri-commands/listing-gone.js'
import type { FileEntry } from '../types'

export const MOVED_LISTING_ID = 'listing-moved'
const DIR = '/moved'

function entry(name: string): FileEntry {
  return {
    name,
    path: `${DIR}/${name}`,
    isDirectory: false,
    isSymlink: false,
    size: 1,
    iconId: 'ext:txt',
  } as FileEntry
}

/** The backend's rows now, at `BACKEND_SEQUENCE`. */
const rows = [entry('alpha.txt'), entry('bravo.txt'), entry('charlie.txt')]

/** The revision the pane applied its selection in. */
export const APPLIED_SEQUENCE = 0
/** The revision the watcher insert moved the backend to. */
export const BACKEND_SEQUENCE = 1
/** The backend row the pane selected `charlie.txt` on, at `APPLIED_SEQUENCE`. */
export const SELECTED_ROW = 1
export const SELECTED_PATH = `${DIR}/charlie.txt`

function refuseUnlessCurrent(listingId: string, expectedSequence: number): void {
  if (listingId !== MOVED_LISTING_ID) throw new ListingLookupFailure({ type: 'gone', listingId })
  if (expectedSequence !== BACKEND_SEQUENCE) throw new ListingLookupFailure({ type: 'changed', listingId })
}

/** The revision-checked entries read (`getSelectionEntries`). */
export function selectionEntries(
  listingId: string,
  _includeHidden: boolean,
  indices: number[],
  expectedSequence: number,
): Promise<FileEntry[]> {
  // A refusal thrown in the executor rejects the promise, as the IPC call would.
  return new Promise((resolve) => {
    refuseUnlessCurrent(listingId, expectedSequence)
    resolve(indices.map((index) => rows[index]))
  })
}

/** The revision-checked paths read (`getSelectionSnapshot`). */
export async function selectionSnapshot(
  listingId: string,
  includeHidden: boolean,
  indices: number[],
  expectedSequence: number,
): Promise<{ paths: string[]; fileCount: number; folderCount: number }> {
  const entries = await selectionEntries(listingId, includeHidden, indices, expectedSequence)
  return { paths: entries.map((e) => e.path), fileCount: entries.length, folderCount: 0 }
}

/** The backend row `charlie.txt` sits on at `BACKEND_SEQUENCE`, once the pane applied the insert. */
export const CAUGHT_UP_ROW = 2

/**
 * The pane's side of the press: its selection (or cursor) on `charlie.txt` at
 * `APPLIED_SEQUENCE`. `catchUp()` is the pane applying the insert's diff: its own
 * selection follows `charlie.txt` to `CAUGHT_UP_ROW` at `BACKEND_SEQUENCE`.
 */
export function createMovedPane(options: { onSelection: boolean }) {
  const state = {
    selected: options.onSelection ? [SELECTED_ROW] : [],
    cursor: options.onSelection ? 0 : SELECTED_ROW,
    sequence: APPLIED_SEQUENCE,
  }
  return {
    getListingId: () => MOVED_LISTING_ID,
    getSelectedIndices: () => [...state.selected],
    getCursorIndex: () => state.cursor,
    hasParentEntry: () => false,
    getLastSequence: () => state.sequence,
    catchUp: () => {
      if (options.onSelection) state.selected = [CAUGHT_UP_ROW]
      else state.cursor = CAUGHT_UP_ROW
      state.sequence = BACKEND_SEQUENCE
    },
  }
}

/**
 * Wraps a checked read so the pane applies the insert's diff right after the
 * first refusal, the way a busy folder's next diff lands while a press waits.
 */
export function catchingUpAfterRefusal<A extends unknown[], R>(
  pane: { catchUp: () => void },
  read: (...args: A) => Promise<R>,
): (...args: A) => Promise<R> {
  return async (...args: A) => {
    try {
      return await read(...args)
    } catch (error) {
      pane.catchUp()
      throw error
    }
  }
}
