/**
 * Pure helper for `FilePane.svelte`'s `hasParent` derivation.
 *
 * For search-results panes, `hasParent` is always `false`: a snapshot pane has
 * no `..` row. Don't fall back to the path comparison `currentPath !==
 * effectiveVolumeRoot` for these panes — a `search-results://sr-N` URL never
 * matches a real volume root, so the comparison returns `true` and
 * `selection.selectAll(hasParent, ...)` then skips index 0 (the off-by-one
 * pinned by the regression test).
 */
export interface HasParentInput {
  /** True when the pane is rendering a search-results snapshot. */
  isSearchResultsView: boolean
  /** The pane's current path (filesystem path OR a virtual-volume URL). */
  currentPath: string
  /** The effective volume root (resolved from the listing event or the prop). */
  effectiveVolumeRoot: string
}

/**
 * Returns whether the pane should render the `..` parent row. Pure: takes a
 * struct and returns a boolean. The contract:
 *   - Search-results panes NEVER have a `..` row (a flat result set has no
 *     parent folder). Returning `false` here is what fixes the round-2 P6
 *     off-by-one in `selectAll`.
 *   - Filesystem root (`'/'`) has no parent.
 *   - When `currentPath === effectiveVolumeRoot` we're at the volume root,
 *     so there's no in-volume parent either.
 *   - Everything else has a parent.
 */
export function computeHasParent(input: HasParentInput): boolean {
  if (input.isSearchResultsView) return false
  if (input.currentPath === '/') return false
  if (input.currentPath === input.effectiveVolumeRoot) return false
  return true
}
