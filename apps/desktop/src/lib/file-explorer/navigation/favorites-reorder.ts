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

/**
 * Pointer-drag reorder math. Given the vertical midpoints of each favorite row (in list order) and
 * the pointer's Y, returns the index the grabbed item should move TO. The pointer drops the item
 * AFTER every row whose midpoint sits above it, so we count midpoints below `pointerY` and place the
 * item just before the first one. The result is already a valid `moveItem(items, from, to)` target:
 * because `moveItem` removes the grabbed item first, an index past `from` still lands correctly.
 *
 * `from` is the grabbed item's current index. Returns `null` when the target equals `from` (a no-op
 * drop), so the caller can treat the gesture as a plain click instead of a reorder.
 */
export function pointerReorderTarget(midpoints: readonly number[], pointerY: number, from: number): number | null {
  if (from < 0 || from >= midpoints.length) return null
  const slot = pointerInsertionSlot(midpoints, pointerY)
  // `slot` is an insertion index into the full list (0..length). Clamp to a valid move target and
  // account for the grabbed item being removed first: a slot past `from` shifts down by one.
  const target = slot > from ? slot - 1 : slot
  const clamped = Math.max(0, Math.min(midpoints.length - 1, target))
  return clamped === from ? null : clamped
}

/**
 * The raw insertion slot for the drop-line CUE: how many rows the pointer sits below, i.e. the gap
 * index in `0..length` where the grabbed item would be inserted. Unlike `pointerReorderTarget`, this
 * is NOT adjusted for the grabbed item being removed first, so it maps directly to a visual gap (slot
 * `k` = the line above row `k`; slot `length` = below the last row). Keeping the cue on the raw slot
 * is why a downward drag highlights the correct gap instead of one row too high.
 */
export function pointerInsertionSlot(midpoints: readonly number[], pointerY: number): number {
  let slot = 0
  for (const mid of midpoints) {
    if (pointerY > mid) slot++
  }
  return slot
}
