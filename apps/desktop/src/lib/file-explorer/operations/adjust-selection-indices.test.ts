import { describe, it, expect } from 'vitest'
import { adjustSelectionIndices } from './adjust-selection-indices'

describe('adjustSelectionIndices', () => {
  describe('no-ops', () => {
    it('returns empty for empty selection', () => {
      expect(adjustSelectionIndices([], [1], [2])).toEqual([])
    })

    it('returns unchanged indices when no removes or adds', () => {
      expect(sorted(adjustSelectionIndices([0, 2, 4], [], []))).toEqual([0, 2, 4])
    })
  })

  describe('add only', () => {
    it('shifts selected index when add is before it', () => {
      // Old [a,b,c], new [X,a,b,c] → adds=[0]
      expect(sorted(adjustSelectionIndices([1], [], [0]))).toEqual([2])
    })

    it('does not shift selected index when add is after it', () => {
      // Old [a,b], new [a,b,X] → adds=[2]
      expect(sorted(adjustSelectionIndices([0], [], [2]))).toEqual([0])
    })

    it('shifts correctly when add is between selected indices', () => {
      // Old [a,b,c], new [a,X,b,c] → adds=[1]. Selected {0,2}
      expect(sorted(adjustSelectionIndices([0, 2], [], [1]))).toEqual([0, 3])
    })

    it('handles multiple adds', () => {
      // Old [a,b,c], new [X,a,Y,b,c] → adds=[0,2]. Selected {1}
      expect(sorted(adjustSelectionIndices([1], [], [0, 2]))).toEqual([3])
    })

    it('handles add at end of listing', () => {
      // Old [a,b], new [a,b,c] → adds=[2]. Selected {0,1}
      expect(sorted(adjustSelectionIndices([0, 1], [], [2]))).toEqual([0, 1])
    })

    it('handles add at beginning of listing', () => {
      // Old [b,c], new [a,b,c] → adds=[0]. Selected {0,1}
      expect(sorted(adjustSelectionIndices([0, 1], [], [0]))).toEqual([1, 2])
    })
  })

  describe('cursor (single element)', () => {
    it('works for single-element arrays (cursor adjustment)', () => {
      // Cursor at index 3, file added at index 1
      expect(adjustSelectionIndices([3], [], [1])).toEqual([4])
    })
  })

  describe('remove only', () => {
    it('shifts selected index when unselected item is removed before it', () => {
      // Old [a,b,c], new [b,c] → removes=[0]. Selected {2}
      expect(sorted(adjustSelectionIndices([2], [0], []))).toEqual([1])
    })

    it('deselects removed index and shifts others', () => {
      // Old [a,b,c,d], new [a,c,d] → removes=[1]. Selected {1,3}
      expect(sorted(adjustSelectionIndices([1, 3], [1], []))).toEqual([2])
    })

    it('does not shift when remove is after selected index', () => {
      // Old [a,b,c], new [a,b] → removes=[2]. Selected {0}
      expect(sorted(adjustSelectionIndices([0], [2], []))).toEqual([0])
    })

    it('handles multiple removes', () => {
      // Old [a,b,c,d,e], new [a,d] → removes=[1,2,4]. Selected {0,3}
      expect(sorted(adjustSelectionIndices([0, 3], [1, 2, 4], []))).toEqual([0, 1])
    })

    it('returns empty when all selected indices are removed', () => {
      expect(adjustSelectionIndices([1, 3], [1, 3], [])).toEqual([])
    })

    it('handles remove at beginning of listing', () => {
      // Old [a,b,c], new [b,c] → removes=[0]. Selected {0,1,2}
      expect(sorted(adjustSelectionIndices([0, 1, 2], [0], []))).toEqual([0, 1])
    })

    it('handles remove at end of listing', () => {
      // Old [a,b,c], new [a,b] → removes=[2]. Selected {0,1,2}
      expect(sorted(adjustSelectionIndices([0, 1, 2], [2], []))).toEqual([0, 1])
    })
  })

  describe('mixed adds and removes', () => {
    it('handles simultaneous add and remove', () => {
      // Old [a,b,c], new [a,X,c] → removes=[1], adds=[1]. Selected {2}
      // Interim: s=2, removedBefore=1 → interim=1. Adds=[1]: 1 <= 1+0 → offset=1. Result: 1+1=2
      expect(sorted(adjustSelectionIndices([2], [1], [1]))).toEqual([2])
    })

    it('deselects when selected item is removed and new item is added at same position', () => {
      // Old [a,b,c], new [a,X,c] → removes=[1], adds=[1]. Selected {1} (b is removed)
      // b was selected, b is removed → deselected. X is a new item, not auto-selected.
      expect(adjustSelectionIndices([1], [1], [1])).toEqual([])
    })

    it('handles add before and remove after selected', () => {
      // Old [a,b,c], new [X,a,b] → removes=[2], adds=[0]. Selected {1}
      expect(sorted(adjustSelectionIndices([1], [2], [0]))).toEqual([2])
    })
  })

  describe('non-contiguous selection', () => {
    it('handles selection with gaps', () => {
      // Old [a,b,c,d,e,f,g,h,i,j], selected {1,5,9}
      // new listing removes nothing, adds [3] → [a,b,c,X,d,e,f,g,h,i,j]
      // Interim: [1,5,9]. Add 3: 3 <= 1? no → emit 1. 3 <= 5? yes, offset=1 → emit 6. 9+1=10 → emit 10.
      expect(sorted(adjustSelectionIndices([1, 5, 9], [], [3]))).toEqual([1, 6, 10])
    })
  })

  describe('large selection', () => {
    it('handles 1000 selected items with small diff correctly and fast', () => {
      // 1000 items selected (0..999), remove indices 100, 500, 900; add at new positions 50, 600
      const selected = Array.from({ length: 1000 }, (_, i) => i)
      const removes = [100, 500, 900]
      const adds = [50, 600]

      const start = performance.now()
      const result = adjustSelectionIndices(selected, removes, adds)
      const elapsed = performance.now() - start

      expect(result.length).toBe(997) // 1000 - 3 removed
      expect(elapsed).toBeLessThan(50) // should be well under 50ms

      // Spot-check: index 0 should become 1 (add at 50 > 0, so offset = 0 initially... let's check)
      // Actually index 0: interim=0, add 50 <= 0? no → result 0. That's before any add.
      expect(result).toContain(0)
      // Index 999 had removes [100,500,900] before it → 3 removed, interim = 996
      // Adds [50,600]: 50 <= 996? yes offset=1. 600 <= 997? yes offset=2. Result = 998.
      expect(result).toContain(998)
    })
  })

  describe('verified examples from spec', () => {
    it('example 1: Old [a,b,c,d,e], new [a,b,X,c,e]', () => {
      // removes=[3] (d), adds=[2] (X). Selected {2,3} → {3}
      const result = sorted(adjustSelectionIndices([2, 3], [3], [2]))
      expect(result).toEqual([3])
    })

    it('example 2: Old [a,b,c,d,e], new [a,X,b,c,d,e]', () => {
      // removes=[], adds=[1]. Selected {0,3} → {0,4}
      const result = sorted(adjustSelectionIndices([0, 3], [], [1]))
      expect(result).toEqual([0, 4])
    })

    it('example 3: Old [a,b,c,d,e,f], new [X,a,c,d,Y,f]', () => {
      // removes=[1,4] (b,e), adds=[0,4] (X,Y). Selected {1,3,5} → {3,5}
      const result = sorted(adjustSelectionIndices([1, 3, 5], [1, 4], [0, 4]))
      expect(result).toEqual([3, 5])
    })
  })
})

function sorted(arr: number[]): number[] {
  return [...arr].sort((a, b) => a - b)
}
