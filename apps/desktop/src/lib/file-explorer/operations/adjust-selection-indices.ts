import type { DiffChange } from '../types'

/**
 * Maps selected indices from an old listing to their positions in a new listing,
 * given the diff (removed and added indices). Operates in backend index space.
 */
export function adjustSelectionIndices(selectedIndices: number[], removes: number[], adds: number[]): number[] {
  if (selectedIndices.length === 0) return []

  const sortedRemoves = [...removes].sort((a, b) => a - b)
  const sortedAdds = [...adds].sort((a, b) => a - b)
  const removeSet = new Set(removes)

  // 1. Drop removed indices, compute interim positions (index in the "removes applied" listing)
  const interim: number[] = []
  for (const s of selectedIndices) {
    if (removeSet.has(s)) continue
    const removedBefore = countLessThan(sortedRemoves, s)
    interim.push(s - removedBefore)
  }

  if (interim.length === 0) return []

  // 2. Adjust for insertions via merge-step
  interim.sort((a, b) => a - b)
  const result: number[] = []
  let addIdx = 0
  let offset = 0

  for (const pos of interim) {
    // Count adds whose new-listing index <= pos + current offset
    while (addIdx < sortedAdds.length && sortedAdds[addIdx] <= pos + offset) {
      offset++
      addIdx++
    }
    result.push(pos + offset)
  }

  return result
}

/** Counts how many elements in a sorted array are strictly less than `target`. */
function countLessThan(sorted: number[], target: number): number {
  let lo = 0
  let hi = sorted.length
  while (lo < hi) {
    const mid = (lo + hi) >>> 1
    if (sorted[mid] < target) lo = mid + 1
    else hi = mid
  }
  return lo
}

/**
 * Where each backend row the diff moved ended up, keyed by the position it left.
 * A `move` is a row whose own sort key changed (an mtime bump under sort-by-date,
 * a size change under sort-by-size), so it jumped to a new position while every
 * other row merely slid over.
 */
export function movesByPreviousIndex(changes: DiffChange[]): Map<number, number> {
  const moves = new Map<number, number>()
  for (const change of changes) {
    if (change.type === 'move' && typeof change.previousIndex === 'number')
      moves.set(change.previousIndex, change.index)
  }
  return moves
}

/**
 * Remaps backend indices across a diff. Rows the diff moved land on their new
 * position by IDENTITY; the rest shift by the removals and insertions around
 * them, counting each move as a removal from where it left and an insertion
 * where it arrived. Rows whose position was removed outright drop out.
 */
export function remapIndicesAcrossDiff(
  indices: number[],
  removes: number[],
  adds: number[],
  moves: ReadonlyMap<number, number>,
): number[] {
  const moved: number[] = []
  const stationary: number[] = []
  for (const index of indices) {
    const destination = moves.get(index)
    if (destination === undefined) stationary.push(index)
    else moved.push(destination)
  }
  return [...adjustSelectionIndices(stationary, removes, adds), ...moved].sort((a, b) => a - b)
}
