/**
 * Pure reorder math for the switcher's Favorites section. Kept separate from `VolumeBreadcrumb.svelte`
 * so the index arithmetic is unit-testable without a DOM. The switcher persists the result via
 * `reorderFavorites(orderedIds)` (bare ids, see `stripFavoritePrefix`).
 */

/**
 * Moves the item at `from` to `to`, shifting the rest. Returns a new array; out-of-range indices or
 * a no-op move (`from === to`) return the input order unchanged (a fresh copy).
 */
export function moveItem<T>(items: readonly T[], from: number, to: number): T[] {
  const next = [...items]
  if (from < 0 || from >= next.length || to < 0 || to >= next.length || from === to) {
    return next
  }
  const [moved] = next.splice(from, 1)
  next.splice(to, 0, moved)
  return next
}

/**
 * The destination index for a keyboard reorder by `delta` (-1 = up, +1 = down), clamped to the
 * list bounds. Returns `null` when the move would leave the array unchanged (already at an edge),
 * so the caller can skip a no-op persist.
 */
export function clampedReorderTarget(from: number, delta: number, length: number): number | null {
  const to = from + delta
  if (from < 0 || from >= length) return null
  if (to < 0 || to >= length) return null
  return to
}
