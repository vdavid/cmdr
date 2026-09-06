/**
 * What a right-click on a search-results row hands the native menu.
 *
 * The snapshot pane can't reuse `pane-pointer::handleContextMenu`: that one
 * resolves the selection through `getPathsAtIndices` against a backend listing,
 * and a snapshot has none. The RULE is the same though, so it lives here as a
 * pure function both the view and its tests can reach.
 */

/** One row of a snapshot pane, as much of it as the menu needs. */
export interface SnapshotRow {
  path: string
}

/**
 * The paths the menu's actions (Copy, Move, Delete, …) should apply to.
 *
 * Finder's rule, and the one a normal pane already follows: a right-click on a
 * row that is part of the selection acts on the whole selection; a right-click
 * anywhere else acts on that row alone. Out-of-range selected indices are
 * dropped, so a selection left over from a longer entries array can't smuggle
 * `undefined` into the menu.
 */
export function snapshotContextMenuPaths(
  clickedPath: string,
  rows: readonly SnapshotRow[],
  selectedIndices: ReadonlySet<number>,
): string[] {
  const selectedPaths: string[] = []
  for (let i = 0; i < rows.length; i++) {
    if (selectedIndices.has(i)) selectedPaths.push(rows[i].path)
  }
  return selectedPaths.includes(clickedPath) ? selectedPaths : [clickedPath]
}

/**
 * The final path segment. The snapshot pane shows the full path in its Name
 * column, so the menu's `Copy {filename}` label and the row's own identity both
 * need this instead of the displayed name.
 *
 * Snapshot rows are absolute paths from the search backend, which is why this
 * doesn't go through `$lib/path/canonical`: that brand exists to keep `~`-rooted
 * and relative input out of exactly this arithmetic, and there is none here.
 */
export function snapshotBasename(path: string): string {
  const idx = path.lastIndexOf('/')
  return idx >= 0 ? path.slice(idx + 1) : path
}
