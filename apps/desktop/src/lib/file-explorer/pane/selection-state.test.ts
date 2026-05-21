import { describe, it, expect, vi } from 'vitest'
import { createSelectionState } from './selection-state.svelte'

describe('createSelectionState', () => {
  it('starts with empty selection', () => {
    const state = createSelectionState()
    expect(state.selectedIndices.size).toBe(0)
    expect(state.anchorIndex).toBeNull()
    expect(state.getSelectedIndices()).toEqual([])
  })

  describe('toggleAt', () => {
    it('selects an unselected index', () => {
      const state = createSelectionState()
      const wasSelected = state.toggleAt(2, false)
      expect(wasSelected).toBe(true)
      expect(state.selectedIndices.has(2)).toBe(true)
    })

    it('deselects a selected index', () => {
      const state = createSelectionState()
      state.toggleAt(2, false)
      const wasSelected = state.toggleAt(2, false)
      expect(wasSelected).toBe(false)
      expect(state.selectedIndices.has(2)).toBe(false)
    })

    it('prevents selecting ".." entry (index 0) when hasParent', () => {
      const state = createSelectionState()
      const wasSelected = state.toggleAt(0, true)
      expect(wasSelected).toBe(false)
      expect(state.selectedIndices.has(0)).toBe(false)
    })

    it('allows selecting index 0 when no parent', () => {
      const state = createSelectionState()
      const wasSelected = state.toggleAt(0, false)
      expect(wasSelected).toBe(true)
      expect(state.selectedIndices.has(0)).toBe(true)
    })

    it('fires onChanged on every mutating toggle (so MCP state stays in sync)', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.toggleAt(2, false) // select
      expect(onChanged).toHaveBeenCalledTimes(1)
      state.toggleAt(2, false) // deselect
      expect(onChanged).toHaveBeenCalledTimes(2)
    })

    it('does not fire onChanged when toggling ".." (no state change)', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.toggleAt(0, true) // parent entry, no-op
      expect(onChanged).not.toHaveBeenCalled()
    })
  })

  describe('selectAll', () => {
    it('selects all indices starting from 0 when no parent', () => {
      const state = createSelectionState()
      state.selectAll(false, 5)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4])
    })

    it('skips index 0 when hasParent', () => {
      const state = createSelectionState()
      state.selectAll(true, 5)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([1, 2, 3, 4])
      expect(state.selectedIndices.has(0)).toBe(false)
    })

    it('clears previous selection before selecting all', () => {
      const state = createSelectionState()
      state.toggleAt(10, false) // out of range for selectAll
      state.selectAll(false, 3)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([0, 1, 2])
    })

    it('calls onChanged callback', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.selectAll(false, 3)
      expect(onChanged).toHaveBeenCalled()
    })
  })

  describe('isAllSelected', () => {
    it('returns true when all selectable indices are selected', () => {
      const state = createSelectionState()
      state.selectAll(false, 3)
      expect(state.isAllSelected(false, 3)).toBe(true)
    })

    it('returns true when all non-parent indices are selected', () => {
      const state = createSelectionState()
      state.selectAll(true, 4)
      expect(state.isAllSelected(true, 4)).toBe(true)
    })

    it('returns false when not all are selected', () => {
      const state = createSelectionState()
      state.toggleAt(1, false)
      expect(state.isAllSelected(false, 3)).toBe(false)
    })

    it('returns false for empty list', () => {
      const state = createSelectionState()
      expect(state.isAllSelected(false, 0)).toBe(false)
    })

    it('returns false for parent-only list (no selectable items)', () => {
      const state = createSelectionState()
      expect(state.isAllSelected(true, 1)).toBe(false)
    })
  })

  describe('getSelectedIndices and setSelectedIndices', () => {
    it('round-trips indices', () => {
      const state = createSelectionState()
      state.setSelectedIndices([3, 5, 7])
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([3, 5, 7])
    })

    it('replaces previous selection', () => {
      const state = createSelectionState()
      state.setSelectedIndices([1, 2, 3])
      state.setSelectedIndices([10, 20])
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([10, 20])
    })

    it('calls onChanged callback', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.setSelectedIndices([1])
      expect(onChanged).toHaveBeenCalled()
    })

    it('returns indices in ascending order regardless of selection sequence', () => {
      // Visible-index ascending is pane-sort order (the listing cache is sorted
      // at fetch time). Write ops process this Vec top-to-bottom, so the user
      // sees the same files copied/moved/deleted first as the ones at the top
      // of the pane, even when they Cmd+clicked them in a non-monotonic order.
      const state = createSelectionState()
      state.toggleAt(15, false)
      state.toggleAt(5, false)
      state.toggleAt(10, false)
      expect(state.getSelectedIndices()).toEqual([5, 10, 15])
    })

    it('returns ascending order even when setSelectedIndices is given non-sorted input', () => {
      const state = createSelectionState()
      state.setSelectedIndices([12, 3, 8, 1])
      expect(state.getSelectedIndices()).toEqual([1, 3, 8, 12])
    })
  })

  describe('clearSelection', () => {
    it('removes all selected indices', () => {
      const state = createSelectionState()
      state.toggleAt(1, false)
      state.toggleAt(2, false)
      state.clearSelection()
      expect(state.selectedIndices.size).toBe(0)
    })

    it('resets anchor', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(2, 1, false)
      state.clearSelection()
      expect(state.anchorIndex).toBeNull()
    })

    it('calls onChanged callback', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.clearSelection()
      expect(onChanged).toHaveBeenCalled()
    })
  })

  describe('deselectAll', () => {
    it('clears selection and calls onChanged', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.toggleAt(1, false)
      state.toggleAt(2, false)
      state.deselectAll()
      expect(state.selectedIndices.size).toBe(0)
      expect(onChanged).toHaveBeenCalled()
    })
  })

  describe('selectRange', () => {
    it('selects indices in range inclusive', () => {
      const state = createSelectionState()
      state.selectRange(2, 5, false)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([2, 3, 4, 5])
    })

    it('works with reversed range', () => {
      const state = createSelectionState()
      state.selectRange(5, 2, false)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([2, 3, 4, 5])
    })

    it('skips index 0 when hasParent', () => {
      const state = createSelectionState()
      state.selectRange(0, 3, true)
      expect(state.selectedIndices.has(0)).toBe(false)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([1, 2, 3])
    })

    it('adds to existing selection', () => {
      const state = createSelectionState()
      state.toggleAt(10, false)
      state.selectRange(2, 4, false)
      expect(state.getSelectedIndices().sort((a, b) => a - b)).toEqual([2, 3, 4, 10])
    })

    it('fires onChanged so MCP state sync runs', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.selectRange(1, 3, false)
      expect(onChanged).toHaveBeenCalled()
    })
  })

  describe('handleShiftMouseNavigation', () => {
    it('sets anchor on first shift-navigation and selects range', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(3, 2, false)
      expect(state.anchorIndex).toBe(2)
      expect(state.selectedIndices.has(2)).toBe(true)
      expect(state.selectedIndices.has(3)).toBe(true)
    })

    it('extends range on subsequent shift-navigation', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(3, 2, false) // anchor=2, range 2-3
      state.handleShiftMouseNavigation(5, 3, false) // extend to 5
      expect(state.selectedIndices.has(2)).toBe(true)
      expect(state.selectedIndices.has(3)).toBe(true)
      expect(state.selectedIndices.has(4)).toBe(true)
      expect(state.selectedIndices.has(5)).toBe(true)
    })

    it('shrinks range when cursor moves back toward anchor', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(5, 2, false) // anchor=2, range 2-5
      state.handleShiftMouseNavigation(3, 5, false) // shrink to 2-3
      expect(state.selectedIndices.has(2)).toBe(true)
      expect(state.selectedIndices.has(3)).toBe(true)
      expect(state.selectedIndices.has(4)).toBe(false)
      expect(state.selectedIndices.has(5)).toBe(false)
    })

    it('empties range when cursor returns to anchor', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(4, 2, false) // anchor=2, range 2-4
      state.handleShiftMouseNavigation(2, 4, false) // back to anchor
      // When cursor == anchor, range is empty
      expect(state.selectedIndices.has(3)).toBe(false)
      expect(state.selectedIndices.has(4)).toBe(false)
    })

    it('enters deselect mode when anchor was already selected', () => {
      const state = createSelectionState()
      // Pre-select some indices
      state.toggleAt(2, false)
      state.toggleAt(3, false)
      state.toggleAt(4, false)
      // Shift-navigate from index 2 (which is selected) -> deselect mode
      state.handleShiftMouseNavigation(4, 2, false)
      expect(state.selectedIndices.has(2)).toBe(false)
      expect(state.selectedIndices.has(3)).toBe(false)
      expect(state.selectedIndices.has(4)).toBe(false)
    })

    it('skips ".." entry (index 0) when hasParent', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(2, 0, true) // anchor at 0
      // Index 0 should be skipped in range
      expect(state.selectedIndices.has(0)).toBe(false)
      expect(state.selectedIndices.has(1)).toBe(true)
      expect(state.selectedIndices.has(2)).toBe(true)
    })

    it('navigates downward from single item', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(1, 0, false) // anchor=0, select 0-1
      expect(state.selectedIndices.has(0)).toBe(true)
      expect(state.selectedIndices.has(1)).toBe(true)
    })

    it('fires onChanged on every step so MCP state sync runs', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.handleShiftMouseNavigation(3, 2, false)
      expect(onChanged).toHaveBeenCalledTimes(1)
      state.handleShiftMouseNavigation(5, 3, false)
      expect(onChanged).toHaveBeenCalledTimes(2)
    })
  })

  describe('clearRangeState', () => {
    it('resets anchor without clearing selection', () => {
      const state = createSelectionState()
      // Shift-navigate from unselected index 1 to 3 (select mode)
      state.handleShiftMouseNavigation(3, 1, false)
      expect(state.anchorIndex).toBe(1)
      expect(state.selectedIndices.size).toBe(3) // 1, 2, 3

      state.clearRangeState()
      expect(state.anchorIndex).toBeNull()
      // Selection should still contain items
      expect(state.selectedIndices.size).toBe(3)
    })
  })

  describe('handleShiftKeyboardNavigation', () => {
    // Toggle-and-fill semantics: signature is (oldCursor, newCursor, overflow, hasParent).
    // The cursor's old item is toggled; items the cursor jumps over are set to that
    // toggled state; the landing item is included only when overflow is true.

    it('Shift+Down 1 step from unselected #5 toggles #5 only, leaves #6 alone', () => {
      const state = createSelectionState()
      state.handleShiftKeyboardNavigation(5, 6, false, false)
      expect(state.selectedIndices.has(5)).toBe(true)
      expect(state.selectedIndices.has(6)).toBe(false)
    })

    it('Shift+Down 1 step from selected #5 toggles #5 off, leaves #6 alone', () => {
      const state = createSelectionState()
      state.setSelectedIndices([5])
      state.handleShiftKeyboardNavigation(5, 6, false, false)
      expect(state.selectedIndices.has(5)).toBe(false)
      expect(state.selectedIndices.has(6)).toBe(false)
    })

    it('Shift+PgDn from #5 to #25 (no overflow): toggle #5, fill #6..#24, #25 untouched', () => {
      const state = createSelectionState()
      // Mix pre-existing states to prove the range is SET, not toggled
      state.setSelectedIndices([7, 10, 25])
      state.handleShiftKeyboardNavigation(5, 25, false, false)
      // #5 toggled from unselected → selected; target state = true; #6..#24 all selected
      expect(state.selectedIndices.has(5)).toBe(true)
      for (let i = 6; i <= 24; i++) {
        expect(state.selectedIndices.has(i)).toBe(true)
      }
      // Landing untouched: still selected (was pre-selected)
      expect(state.selectedIndices.has(25)).toBe(true)
    })

    it('Shift+PgDn with overflow: landing IS included', () => {
      const state = createSelectionState()
      // Cursor on #5 (selected), intended PgDn would land on #25 but clamps to #23
      state.setSelectedIndices([5, 10, 23])
      state.handleShiftKeyboardNavigation(5, 23, true, false)
      // Target = false (toggled from selected); fill #6..#23 → all deselected
      expect(state.selectedIndices.has(5)).toBe(false)
      for (let i = 6; i <= 23; i++) {
        expect(state.selectedIndices.has(i)).toBe(false)
      }
    })

    it('Shift+Up at boundary #0 (no parent, overflow): toggles #0', () => {
      const state = createSelectionState()
      state.handleShiftKeyboardNavigation(0, 0, true, false)
      expect(state.selectedIndices.has(0)).toBe(true)
    })

    it('Shift+Up at #0 == ".." (hasParent, overflow): no-op', () => {
      const state = createSelectionState()
      state.handleShiftKeyboardNavigation(0, 0, true, true)
      expect(state.selectedIndices.has(0)).toBe(false)
      expect(state.selectedIndices.size).toBe(0)
    })

    it('Shift+End from #0 == ".." (hasParent, overflow): fill #1..last with target=true', () => {
      const state = createSelectionState()
      state.handleShiftKeyboardNavigation(0, 10, true, true)
      expect(state.selectedIndices.has(0)).toBe(false) // ".." stays
      for (let i = 1; i <= 10; i++) {
        expect(state.selectedIndices.has(i)).toBe(true)
      }
    })

    it('Shift+PgDn from #0 == ".." (hasParent, no overflow): fill #1..#9 with target=true, #10 untouched', () => {
      const state = createSelectionState()
      state.handleShiftKeyboardNavigation(0, 10, false, true)
      expect(state.selectedIndices.has(0)).toBe(false)
      for (let i = 1; i <= 9; i++) {
        expect(state.selectedIndices.has(i)).toBe(true)
      }
      expect(state.selectedIndices.has(10)).toBe(false)
    })

    it('Shift+Home from #5 (hasParent, overflow): toggle #5, fill #4..#1 with that state, skip #0', () => {
      const state = createSelectionState()
      state.handleShiftKeyboardNavigation(5, 0, true, true)
      expect(state.selectedIndices.has(5)).toBe(true)
      for (let i = 1; i <= 4; i++) {
        expect(state.selectedIndices.has(i)).toBe(true)
      }
      expect(state.selectedIndices.has(0)).toBe(false) // ".." skipped even on overflow
    })

    it('Shift+End from selected #5 (overflow): toggle #5 off, fill #6..last off', () => {
      const state = createSelectionState()
      state.setSelectedIndices([5, 6, 7, 8, 9])
      state.handleShiftKeyboardNavigation(5, 9, true, false)
      for (let i = 5; i <= 9; i++) {
        expect(state.selectedIndices.has(i)).toBe(false)
      }
    })

    it('asymmetry: Shift+Down 3× then Shift+Up 3× does NOT restore the start (intentional)', () => {
      const state = createSelectionState()
      // Start: nothing selected, cursor on #5. Simulate Shift+Down ×3 → cursor ends on #8.
      state.handleShiftKeyboardNavigation(5, 6, false, false) // toggle #5 → on
      state.handleShiftKeyboardNavigation(6, 7, false, false) // toggle #6 → on
      state.handleShiftKeyboardNavigation(7, 8, false, false) // toggle #7 → on
      expect([5, 6, 7].every((i) => state.selectedIndices.has(i))).toBe(true)
      expect(state.selectedIndices.has(8)).toBe(false)
      // Now Shift+Up ×3 → cursor returns to #5
      state.handleShiftKeyboardNavigation(8, 7, false, false) // toggle #8 → on
      state.handleShiftKeyboardNavigation(7, 6, false, false) // toggle #7 → off
      state.handleShiftKeyboardNavigation(6, 5, false, false) // toggle #6 → off
      // Items #5 (still on), #6, #7 deselected, #8 newly selected.
      expect(state.selectedIndices.has(5)).toBe(true)
      expect(state.selectedIndices.has(6)).toBe(false)
      expect(state.selectedIndices.has(7)).toBe(false)
      expect(state.selectedIndices.has(8)).toBe(true)
    })

    it('fires onChanged once per call', () => {
      const onChanged = vi.fn()
      const state = createSelectionState({ onChanged })
      state.handleShiftKeyboardNavigation(5, 10, false, false)
      expect(onChanged).toHaveBeenCalledTimes(1)
      state.handleShiftKeyboardNavigation(10, 9, false, false)
      expect(onChanged).toHaveBeenCalledTimes(2)
    })
  })

  describe('edge cases', () => {
    it('handles single-item mouse shift-navigation (anchor == end)', () => {
      const state = createSelectionState()
      state.handleShiftMouseNavigation(5, 5, false) // anchor and end are same
      // When newEnd === anchor, range is empty
      expect(state.selectedIndices.size).toBe(0)
    })

    it('handles toggling the same index multiple times', () => {
      const state = createSelectionState()
      state.toggleAt(3, false)
      state.toggleAt(3, false)
      state.toggleAt(3, false)
      expect(state.selectedIndices.has(3)).toBe(true)
      expect(state.selectedIndices.size).toBe(1)
    })

    it('handles empty setSelectedIndices', () => {
      const state = createSelectionState()
      state.toggleAt(1, false)
      state.setSelectedIndices([])
      expect(state.selectedIndices.size).toBe(0)
    })
  })
})
