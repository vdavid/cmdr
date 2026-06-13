import { describe, it, expect } from 'vitest'
import { moveItem, clampedReorderTarget } from './favorites-reorder'

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
