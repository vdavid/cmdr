/**
 * Shared scaffolding for the rename-flow tests.
 *
 * The `vi.mock` calls stay in each test file (they're hoisted per file, and each
 * file wants its own spies); what lives here is the pane the flow talks to: a
 * small fake listing, the deps bag, and the deferred-promise helper that holds a
 * save in flight.
 */

import { vi } from 'vitest'
import { createRenameFlow } from './rename-flow.svelte'
import { createRenameState } from '../rename/rename-state.svelte'

export type Entry = { name: string; path: string; isDirectory: boolean }

export const PASTED: Entry = { name: 'pasted.txt', path: '/dir/pasted.txt', isDirectory: false }
const PARENT: Entry = { name: '..', path: '/', isDirectory: true }

/** A promise the test resolves by hand, to hold a save "in flight". */
export function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

/**
 * A listing the chained rename step can walk: row 0 is `..`, the named files
 * follow. It tracks its own cursor, so a test can assert where a chain ended up
 * and that the editor was scrolled along with it.
 *
 * It keeps the pane's two clocks apart. `rows` is the listing itself, which the
 * cursor is reconciled against the moment a diff lands; the loaded window the
 * rows are READ from is a snapshot that only catches up when the pane's
 * throttled refetch runs. `landRenameSortingToTop` moves the first and leaves
 * the second behind, which is the state a chain runs into every time one of its
 * own renames lands.
 */
export function chainListing(names: string[], startIndex = 1) {
  const rows: Entry[] = [PARENT, ...names.map((name) => ({ name, path: `/dir/${name}`, isDirectory: false }))]
  let windowRows: Entry[] = [...rows]
  const entryUnderCursorAtStart = rows[startIndex]
  const unloaded = new Set<number>()
  let cursorIndex = startIndex
  const moves: number[] = []
  return {
    /** Rows the cursor visited, in order. */
    moves,
    /** Makes a row unreadable from the loaded window, as if it had scrolled out of it. */
    unload(index: number) {
      unloaded.add(index)
    },
    /**
     * A chained rename lands on a name that sorts to the top of the directory:
     * the row leaves from below the cursor and comes back above it.
     *
     * The cursor is reconciled straight away, the way `listing-diff-sync` does
     * it (a row added above lifts it, a row removed below leaves it), while the
     * loaded window keeps serving what it fetched before.
     */
    landRenameSortingToTop(name: string, newName: string) {
      const from = rows.findIndex((row) => row.name === name)
      rows.splice(from, 1)
      const addedAt = 1
      rows.splice(addedAt, 0, { name: newName, path: `/dir/${newName}`, isDirectory: false })
      if (from <= cursorIndex) cursorIndex -= 1
      if (addedAt <= cursorIndex) cursorIndex += 1
    },
    /** The throttled refetch lands and the window catches up with the listing. */
    windowCatchesUp() {
      windowRows = [...rows]
    },
    /** What the backend answers for a row. Backend indices skip the `..` row. */
    backendRowAt(backendIndex: number) {
      return rows[backendIndex + 1] ?? null
    },
    /**
     * What `entryUnderCursor` reports: the row the cursor sat on when the pane
     * last finished its async read. It never updates, which is what a hop has to
     * survive — an activation that trusted it would rename the file just left.
     */
    staleEntryUnderCursor: () => entryUnderCursorAtStart,
    /**
     * What `entryUnderCursor` reports once the pane's read has caught up: the
     * row the cursor is actually on. This is what a rename started outside a
     * chain (F2, the menu) opens on.
     */
    entryUnderCursor: () => windowRows[cursorIndex],
    deps: {
      getCursorIndex: () => cursorIndex,
      getEffectiveTotalCount: () => rows.length,
      // Backend counts skip the `..` row, so this is what a sibling-name read pages.
      getTotalCount: () => names.length,
      getHasParent: () => true,
      getEntryAt: (index: number) => (unloaded.has(index) ? undefined : windowRows[index]),
      moveCursorTo: (index: number) => {
        cursorIndex = index
        moves.push(index)
      },
    },
  }
}

/** A pane with nothing to step to and nothing to read, for the tests that never chain. */
const NO_NEIGHBOURS = {
  getCursorIndex: () => 0,
  getEffectiveTotalCount: () => 0,
  // 0 rows → the sibling-name read answers [] without an IPC call.
  getTotalCount: () => 0,
  getHasParent: () => false,
  getEntryAt: () => undefined,
  moveCursorTo: () => {},
}

export function buildFlow(
  getEntry: () => Entry | undefined = () => PASTED,
  showHiddenFiles = true,
  listingDeps: ReturnType<typeof chainListing>['deps'] | typeof NO_NEIGHBOURS = NO_NEIGHBOURS,
) {
  const rename = createRenameState()
  const onRequestFocus = vi.fn()
  const flow = createRenameFlow({
    rename,
    paneId: 'left',
    getListingId: () => 'lst-1',
    getIncludeHidden: () => false,
    getCurrentPath: () => '/dir',
    getShowHiddenFiles: () => showHiddenFiles,
    getVolumeId: () => 'root',
    getEntryUnderCursor: () => getEntry() as never,
    onRequestFocus,
    ...listingDeps,
    getEntryAt: (index: number) => listingDeps.getEntryAt(index) as never,
  })
  return { rename, flow, onRequestFocus }
}
