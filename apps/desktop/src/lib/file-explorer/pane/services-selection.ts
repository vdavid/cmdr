/**
 * What the focused pane tells the backend `Cmdr > Services` should act on.
 *
 * Pure, so the rule can be pinned without a pane: the effect that pushes it lives
 * in `FilePane.svelte`, and the state it feeds is
 * `apps/desktop/src-tauri/src/services_menu/`.
 */

import type { ServicesSelection } from '$lib/tauri-commands'

/** Where the pane keeps its rows, which decides how a selection can travel. */
export type PaneRowSource =
  /**
   * A normal pane: rows live in the backend's listing cache, so selected rows
   * travel as INDICES and the backend resolves them when a service asks.
   */
  | { kind: 'listing'; listingId: string; includeHidden: boolean; hasParent: boolean }
  /**
   * The search-results snapshot: its rows live here and there's no cached listing
   * to index into, so they travel as paths. An accessor rather than the whole
   * array, so a push over a large result set costs one lookup per SELECTED row
   * instead of a copy of every row.
   */
  | { kind: 'snapshot'; pathAt: (index: number) => string | undefined }

export interface ServicesSelectionArgs {
  /**
   * Whether this pane's rows are ordinary OS paths. `false` for a phone, an
   * archive's insides, the `.git` portal, and the host list, and then the pane
   * offers nothing: a file URL for a row with no file behind it gives a service
   * something it can't read.
   */
  rowsAreOsVisible: boolean
  /** The cursor row's path, or `null` on the `..` row and in an empty pane. */
  cursorPath: string | null
  /** The pane's selected row indices, `..` offset included, as the pane holds them. */
  selectedIndices: readonly number[]
  rows: PaneRowSource
}

/**
 * Builds the push payload, applying Finder's rule at the edge: with a selection,
 * act on the selection; with none, act on the cursor row. An empty selection
 * becomes `rows: null` so "nothing selected" has exactly one spelling on the wire.
 */
export function servicesSelectionForPane(args: ServicesSelectionArgs): ServicesSelection {
  if (!args.rowsAreOsVisible) return { cursorPath: '', rows: null }
  return { cursorPath: args.cursorPath ?? '', rows: selectedRows(args) }
}

function selectedRows({ selectedIndices, rows }: ServicesSelectionArgs): ServicesSelection['rows'] {
  if (selectedIndices.length === 0) return null
  if (rows.kind === 'snapshot') {
    // Out-of-range indices are dropped rather than carried: a selection left over
    // from a longer result set would otherwise put `undefined` on the wire.
    const paths = selectedIndices.map((index) => rows.pathAt(index)).filter(isPath)
    return paths.length > 0 ? { kind: 'paths', paths } : null
  }
  // Mid-navigation the pane can hold indices with nothing to resolve them
  // against yet. The cursor row is the honest answer until the listing lands.
  if (!rows.listingId) return null
  return {
    kind: 'listing',
    listingId: rows.listingId,
    indices: [...selectedIndices],
    includeHidden: rows.includeHidden,
    hasParent: rows.hasParent,
  }
}

/** TS has no `noUncheckedIndexedAccess` here, so the bounds guard is at runtime. */
function isPath(value: string | undefined): value is string {
  return typeof value === 'string' && value.length > 0
}
