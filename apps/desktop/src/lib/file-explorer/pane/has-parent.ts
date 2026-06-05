/**
 * Pure helper for `FilePane.svelte`'s `hasParent` derivation.
 *
 * The snapshot rule comes in as the `hasParentRow` capability
 * (`VolumeCapabilities.hasParentRow`, `false` for the search-results kind), NOT
 * a `volumeId === 'search-results'` string compare (invariant A6). A snapshot
 * pane has no `..` row, and the path comparison `currentPath !==
 * effectiveVolumeRoot` can't catch it — a `search-results://sr-N` URL never
 * matches a real volume root, so without `hasParentRow` the comparison returns
 * `true` and `selection.selectAll(hasParent, ...)` then skips index 0 (the
 * off-by-one pinned by the regression test).
 *
 * `hasParentRow` folds ONLY the snapshot rule; it is NOT a complete has-parent
 * answer. The two PATH comparisons stay here: a `local` pane at `/`, or any pane
 * sitting on its volume root, has no `..` despite `hasParentRow: true`. The real
 * answer is `hasParentRow && currentPath !== '/' && currentPath !== root` (L5 —
 * this stays coupled to `isCrossVolumeNavigation`, the snapshot no-`..` rule).
 */
export interface HasParentInput {
  /**
   * The pane kind's `hasParentRow` capability: `false` for the search-results
   * (snapshot) kind, `true` otherwise. Folds the snapshot rule only.
   */
  hasParentRow: boolean
  /** The pane's current path (filesystem path OR a virtual-volume URL). */
  currentPath: string
  /** The effective volume root (resolved from the listing event or the prop). */
  effectiveVolumeRoot: string
}

/**
 * Returns whether the pane should render the `..` parent row. Pure: takes a
 * struct and returns a boolean. The contract:
 *   - Kinds without a `..` row (the snapshot kind, `hasParentRow: false`) NEVER
 *     have a parent row. Returning `false` here is what fixes the round-2 P6
 *     off-by-one in `selectAll`.
 *   - Filesystem root (`'/'`) has no parent.
 *   - When `currentPath === effectiveVolumeRoot` we're at the volume root,
 *     so there's no in-volume parent either.
 *   - Everything else has a parent.
 */
export function computeHasParent(input: HasParentInput): boolean {
  if (!input.hasParentRow) return false
  if (input.currentPath === '/') return false
  if (input.currentPath === input.effectiveVolumeRoot) return false
  return true
}
