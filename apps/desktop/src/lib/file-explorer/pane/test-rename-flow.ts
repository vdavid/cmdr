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

/** A file, as a row: the shape both listings and the loaded window hold. */
function row(name: string): Entry {
  return { name, path: `/dir/${name}`, isDirectory: false }
}

/**
 * A listing the chained rename step can walk: row 0 is `..`, the named files
 * follow. It tracks its own cursor, so a test can assert where a chain ended up
 * and that the editor was scrolled along with it.
 *
 * It keeps the pane's three clocks apart, because a chain runs with all three
 * showing different things:
 *
 * - `backendRows` is the backend's own listing cache, which a rename mutates the
 *   moment it lands there, ahead of the `directory-diff` it will be announced
 *   with (`renameOnBackend`).
 * - `rows` is the frontend listing the cursor is reconciled against, which
 *   catches up when that diff arrives (`landRenameSortingToTop`).
 * - `windowRows` is the loaded window the rows are READ from, a snapshot that
 *   only catches up when the pane's throttled refetch runs (`windowCatchesUp`).
 */
export function chainListing(names: string[], startIndex = 1) {
  const rows: Entry[] = [PARENT, ...names.map(row)]
  let windowRows: Entry[] = [...rows]
  const backendRows: Entry[] = names.map(row)
  const entryUnderCursorAtStart = rows[startIndex]
  const unloaded = new Set<number>()
  let cursorIndex = startIndex
  const moves: number[] = []
  /** Puts a renamed row back where its new name sorts, the way the backend does. */
  function resortOnBackend(name: string, newName: string) {
    const from = backendRows.findIndex((entry) => entry.name === name)
    if (from === -1) return
    backendRows.splice(from, 1)
    const goesBefore = backendRows.findIndex((entry) => entry.name > newName)
    backendRows.splice(goesBefore === -1 ? backendRows.length : goesBefore, 0, row(newName))
  }
  return {
    /** Rows the cursor visited, in order. */
    moves,
    /** Makes a row unreadable from the loaded window, as if it had scrolled out of it. */
    unload(index: number) {
      unloaded.add(index)
    },
    /** Makes the whole window unreadable: the chain has outrun the pane's prefetch. */
    unloadWindow() {
      for (let index = 0; index <= names.length; index++) unloaded.add(index)
    },
    /**
     * A chained rename lands in the BACKEND's listing, which happens the moment
     * the rename does. Its `directory-diff` waits out a 50 ms coalescing window,
     * so a chain stepping faster than that runs with the backend a rename or two
     * ahead of everything the frontend has been told about.
     */
    renameOnBackend(name: string, newName: string) {
      resortOnBackend(name, newName)
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
      resortOnBackend(name, newName)
      const from = rows.findIndex((entry) => entry.name === name)
      rows.splice(from, 1)
      const addedAt = 1
      rows.splice(addedAt, 0, row(newName))
      if (from <= cursorIndex) cursorIndex -= 1
      if (addedAt <= cursorIndex) cursorIndex += 1
    },
    /** The throttled refetch lands and the window catches up with the listing. */
    windowCatchesUp() {
      windowRows = [...rows]
    },
    /** What the backend answers for a row. Backend indices skip the `..` row. */
    backendRowAt(backendIndex: number) {
      return backendRows[backendIndex] ?? null
    },
    /** What the backend answers for the row beside the one named `name`. */
    backendRowNextTo(name: string, direction: 'previous' | 'next') {
      const at = backendRows.findIndex((entry) => entry.name === name)
      if (at === -1) return null
      return backendRows[direction === 'next' ? at + 1 : at - 1] ?? null
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
      indexOfEntry: (path: string) => {
        const at = windowRows.findIndex((entry) => entry.path === path)
        return at === -1 || unloaded.has(at) ? undefined : at
      },
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
  indexOfEntry: () => undefined,
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
    indexOfEntry: (path: string) => listingDeps.indexOfEntry(path),
  })
  return { rename, flow, onRequestFocus }
}
