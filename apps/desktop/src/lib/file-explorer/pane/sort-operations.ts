/**
 * Sort orchestration for the two panes: column-click cycling, order toggles, the
 * atomic MCP set, and the re-sort-both-panes hook the directory-sort-mode effect
 * drives. Lifted out of `DualPaneExplorer` so the component keeps only the
 * one-line `export` delegates plus the reactive `$effect` that calls
 * `resortPaneWithCurrentSort` when the sort mode changes (that stays component
 * wiring — a reactive read + init gate, not logic).
 *
 * The pure helpers stay in `sorting-handlers.ts` (`getNewSortOrder`,
 * `applySortResult`, `collectSortState`); this factory is the IPC-touching
 * orchestration over them, in the `clipboard-operations` / `file-operation-commands`
 * factory shape: pass a `SortOperationsDeps` of live store reads/writes, get back
 * the command bodies.
 */

import { resortListing } from '$lib/tauri-commands'
import { getDirectorySortMode } from '$lib/settings/reactive-settings.svelte'
import { getSnapshot, snapshotIdFromPanePath } from '$lib/search/snapshot-store.svelte'
import { nextSnapshotSort, sortSnapshot } from '$lib/search/snapshot-sort.svelte'
import type { SortColumn, SortOrder } from '../types'
import { defaultSortOrders } from '../types'
import type { FilePaneAPI } from './types'
import { getNewSortOrder, applySortResult, collectSortState } from './sorting-handlers'

export interface SortOperationsDeps {
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getPaneSort: (pane: 'left' | 'right') => { sortBy: SortColumn; sortOrder: SortOrder }
  setPaneSort: (pane: 'left' | 'right', sortBy: SortColumn, sortOrder: SortOrder) => void
  getShowHiddenFiles: () => boolean
  getFocusedPane: () => 'left' | 'right'
}

export interface SortOperations {
  /** Column-header click: cycles order on the same column, applies the column's
   *  default order on a new column, re-sorts, and commits the new sort to the
   *  store (persistence reacts). Cancels rename + type-to-jump first (a re-sort
   *  invalidates the listing's index space). */
  handleSortChange: (pane: 'left' | 'right', newColumn: SortColumn) => Promise<void>
  /** Re-sorts one pane in place with its current column/order but the current
   *  `directorySortMode`. Driven by the component's sort-mode `$effect`. */
  resortPaneWithCurrentSort: (pane: 'left' | 'right') => Promise<void>
  /** Sets column + order atomically for a pane (MCP `sort`, race-free). */
  setSort: (column: SortColumn, order: 'asc' | 'desc', pane: 'left' | 'right') => Promise<void>
  /** Sets the sort column for a pane (defaults to focused). Palette entry. */
  setSortColumn: (column: SortColumn, pane?: 'left' | 'right') => void
  /** Sets the sort order for a pane (defaults to focused). Palette entry. */
  setSortOrder: (order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') => void
}

export function createSortOperations(deps: SortOperationsDeps): SortOperations {
  /**
   * The snapshot id a pane is showing, or `null` for an ordinary pane.
   *
   * A search-results pane sorts its SNAPSHOT, in the store, because every consumer
   * resolves the index the user sees against `snapshot.entries[i]`. Its tab's
   * `sortBy` / `sortOrder` belong to the folder the user came from and must not be
   * touched, so this branch returns before any `setPaneSort` or `resortListing`.
   * Reads the pane's own path rather than a capability, because the id is in it.
   */
  function snapshotPaneId(pane: 'left' | 'right'): string | null {
    const currentPath = deps.getPaneRef(pane)?.getCurrentPath()
    return currentPath === undefined ? null : snapshotIdFromPanePath(currentPath)
  }

  async function handleSortChange(pane: 'left' | 'right', newColumn: SortColumn): Promise<void> {
    // Cancel any active rename on the affected pane (sort invalidates indices)
    deps.getPaneRef(pane)?.cancelRename()
    // Re-sort changes the listing's index space; any in-flight type-to-jump
    // match would land on the wrong row.
    deps.getPaneRef(pane)?.clearJumpState()

    const snapshotId = snapshotPaneId(pane)
    if (snapshotId !== null) {
      // The same tri-state a header click runs, so the keyboard and the mouse
      // can't disagree about where the third press lands.
      await sortSnapshot(snapshotId, nextSnapshotSort(getSnapshot(snapshotId)?.sort ?? null, newColumn))
      return
    }

    const paneRef = deps.getPaneRef(pane)
    const listingId = paneRef?.getListingId()
    if (!listingId) return

    const { sortBy, sortOrder } = deps.getPaneSort(pane)
    const newOrder = newColumn === sortBy ? getNewSortOrder(newColumn, sortBy, sortOrder) : defaultSortOrders[newColumn]

    const sortState = collectSortState(paneRef)
    const result = await resortListing(
      listingId,
      newColumn,
      newOrder,
      sortState.cursorFilename,
      deps.getShowHiddenFiles(),
      sortState.backendSelectedIndices,
      sortState.allSelected,
      getDirectorySortMode(),
    )

    deps.setPaneSort(pane, newColumn, newOrder)
    // Persistence (saveAppStatus sortBy + the pane's tab set) fires from the
    // single subscriber's per-pane effect, which reacts to this store change.
    applySortResult(paneRef, result, sortState.hasParent)
  }

  async function resortPaneWithCurrentSort(pane: 'left' | 'right'): Promise<void> {
    const snapshotId = snapshotPaneId(pane)
    if (snapshotId !== null) {
      // `directorySortMode` crosses to the backend comparator on every snapshot
      // sort too, so a snapshot pane already ordered by a column has to re-ask
      // when the setting flips. One in the ranked order has nothing to redo.
      const sort = getSnapshot(snapshotId)?.sort
      if (sort) await sortSnapshot(snapshotId, sort)
      return
    }

    const paneRef = deps.getPaneRef(pane)
    const listingId = paneRef?.getListingId()
    if (!listingId) return

    const { sortBy, sortOrder } = deps.getPaneSort(pane)
    const sortState = collectSortState(paneRef)
    const result = await resortListing(
      listingId,
      sortBy,
      sortOrder,
      sortState.cursorFilename,
      deps.getShowHiddenFiles(),
      sortState.backendSelectedIndices,
      sortState.allSelected,
      getDirectorySortMode(),
    )
    applySortResult(paneRef, result, sortState.hasParent)
  }

  async function setSort(column: SortColumn, order: 'asc' | 'desc', pane: 'left' | 'right'): Promise<void> {
    const newOrderAsked: SortOrder = order === 'asc' ? 'ascending' : 'descending'
    const snapshotId = snapshotPaneId(pane)
    if (snapshotId !== null) {
      // The atomic set names both halves, so there is no cycle to run and no way
      // to ask for the ranked order through it. An agent restores that by opening
      // the snapshot again.
      await sortSnapshot(snapshotId, { column, order: newOrderAsked })
      return
    }

    const paneRef = deps.getPaneRef(pane)
    const listingId = paneRef?.getListingId()
    if (!listingId) return

    const newOrder: SortOrder = newOrderAsked

    const sortState = collectSortState(paneRef)
    const result = await resortListing(
      listingId,
      column,
      newOrder,
      sortState.cursorFilename,
      deps.getShowHiddenFiles(),
      sortState.backendSelectedIndices,
      sortState.allSelected,
      getDirectorySortMode(),
    )

    deps.setPaneSort(pane, column, newOrder)
    // Sort persistence (app-status sortBy + tab set) fires from the subscriber.
    applySortResult(paneRef, result, sortState.hasParent)
  }

  function setSortColumn(column: SortColumn, pane?: 'left' | 'right'): void {
    void handleSortChange(pane ?? deps.getFocusedPane(), column)
  }

  function setSortOrder(order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right'): void {
    const targetPane = pane ?? deps.getFocusedPane()
    // On a snapshot pane the live order is the SNAPSHOT's, and a pane in the
    // ranked order has no column to re-point, so there is nothing to flip.
    const snapshotId = snapshotPaneId(targetPane)
    const snapshotSort = snapshotId === null ? null : (getSnapshot(snapshotId)?.sort ?? null)
    if (snapshotId !== null && snapshotSort === null) return
    const { sortOrder: currentOrder, sortBy: currentColumn } = snapshotSort
      ? { sortOrder: snapshotSort.order, sortBy: snapshotSort.column }
      : deps.getPaneSort(targetPane)

    let newOrder: SortOrder
    if (order === 'toggle') {
      newOrder = currentOrder === 'ascending' ? 'descending' : 'ascending'
    } else {
      newOrder = order === 'asc' ? 'ascending' : 'descending'
    }

    if (newOrder === currentOrder) return
    // A snapshot pane takes the order straight: routing it through the cycle
    // would land the third press on the ranked order instead of the flip asked
    // for. An ordinary pane re-applies by pretending to click the same column,
    // which is what triggers the toggle in the handler.
    if (snapshotId !== null) void sortSnapshot(snapshotId, { column: currentColumn, order: newOrder })
    else void handleSortChange(targetPane, currentColumn)
  }

  return { handleSortChange, resortPaneWithCurrentSort, setSort, setSortColumn, setSortOrder }
}
