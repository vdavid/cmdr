import { describe, it, expect } from 'vitest'
import { moveItem, clampedReorderTarget, pointerReorderTarget } from './favorites-reorder'

describe('moveItem', () => {
  it('moves an item later in the list, shifting the rest', () => {
    expect(moveItem(['a', 'b', 'c', 'd'], 0, 2)).toEqual(['b', 'c', 'a', 'd'])
  })

  it('moves an item earlier in the list', () => {
    expect(moveItem(['a', 'b', 'c', 'd'], 3, 1)).toEqual(['a', 'd', 'b', 'c'])
  })

  it('returns an unchanged copy for a no-op move', () => {
    const input = ['a', 'b', 'c']
    const out = moveItem(input, 1, 1)
    expect(out).toEqual(input)
    expect(out).not.toBe(input)
  })

  it('returns an unchanged copy for out-of-range indices', () => {
    expect(moveItem(['a', 'b'], -1, 0)).toEqual(['a', 'b'])
    expect(moveItem(['a', 'b'], 0, 5)).toEqual(['a', 'b'])
  })
})

describe('clampedReorderTarget', () => {
  it('returns the target one step up / down', () => {
    expect(clampedReorderTarget(2, -1, 4)).toBe(1)
    expect(clampedReorderTarget(1, 1, 4)).toBe(2)
  })

  it('returns null at the top edge moving up', () => {
    expect(clampedReorderTarget(0, -1, 4)).toBeNull()
  })

  it('returns null at the bottom edge moving down', () => {
    expect(clampedReorderTarget(3, 1, 4)).toBeNull()
  })

  it('returns null for an out-of-range source', () => {
    expect(clampedReorderTarget(9, -1, 4)).toBeNull()
  })
})

describe('pointerReorderTarget', () => {
  // Four rows, 20px tall, stacked from y=0: midpoints at 10, 30, 50, 70.
  const midpoints = [10, 30, 50, 70]

  it('returns null when the pointer stays over the grabbed row (a click, not a drag)', () => {
    // Grab row 0, pointer still in row 0's band.
    expect(pointerReorderTarget(midpoints, 5, 0)).toBeNull()
    // Grab row 2, pointer between its own midpoint neighbors.
    expect(pointerReorderTarget(midpoints, 45, 2)).toBeNull()
  })

  it('moves a top item down past lower rows', () => {
    // Grab row 0, drag down past rows 1 and 2 (pointer below midpoint 50, above 70).
    expect(pointerReorderTarget(midpoints, 60, 0)).toBe(2)
  })

  it('moves a bottom item up to the top', () => {
    // Grab row 3, drag above the first midpoint.
    expect(pointerReorderTarget(midpoints, 5, 3)).toBe(0)
  })

  it('clamps a pointer dragged past the bottom edge to the last index', () => {
    expect(pointerReorderTarget(midpoints, 999, 0)).toBe(3)
  })

  it('clamps a pointer dragged above the top edge to index 0', () => {
    expect(pointerReorderTarget(midpoints, -999, 3)).toBe(0)
  })

  it('returns null for an out-of-range grabbed index', () => {
    expect(pointerReorderTarget(midpoints, 50, 9)).toBeNull()
  })
})
