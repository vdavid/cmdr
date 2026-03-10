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
            state.handleShiftNavigation(2, 1, false)
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
    })

    describe('handleShiftNavigation', () => {
        it('sets anchor on first shift-navigation and selects range', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(3, 2, false)
            expect(state.anchorIndex).toBe(2)
            expect(state.selectedIndices.has(2)).toBe(true)
            expect(state.selectedIndices.has(3)).toBe(true)
        })

        it('extends range on subsequent shift-navigation', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(3, 2, false) // anchor=2, range 2-3
            state.handleShiftNavigation(5, 3, false) // extend to 5
            expect(state.selectedIndices.has(2)).toBe(true)
            expect(state.selectedIndices.has(3)).toBe(true)
            expect(state.selectedIndices.has(4)).toBe(true)
            expect(state.selectedIndices.has(5)).toBe(true)
        })

        it('shrinks range when cursor moves back toward anchor', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(5, 2, false) // anchor=2, range 2-5
            state.handleShiftNavigation(3, 5, false) // shrink to 2-3
            expect(state.selectedIndices.has(2)).toBe(true)
            expect(state.selectedIndices.has(3)).toBe(true)
            expect(state.selectedIndices.has(4)).toBe(false)
            expect(state.selectedIndices.has(5)).toBe(false)
        })

        it('empties range when cursor returns to anchor', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(4, 2, false) // anchor=2, range 2-4
            state.handleShiftNavigation(2, 4, false) // back to anchor
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
            state.handleShiftNavigation(4, 2, false)
            expect(state.selectedIndices.has(2)).toBe(false)
            expect(state.selectedIndices.has(3)).toBe(false)
            expect(state.selectedIndices.has(4)).toBe(false)
        })

        it('skips ".." entry (index 0) when hasParent', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(2, 0, true) // anchor at 0
            // Index 0 should be skipped in range
            expect(state.selectedIndices.has(0)).toBe(false)
            expect(state.selectedIndices.has(1)).toBe(true)
            expect(state.selectedIndices.has(2)).toBe(true)
        })

        it('navigates downward from single item', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(1, 0, false) // anchor=0, select 0-1
            expect(state.selectedIndices.has(0)).toBe(true)
            expect(state.selectedIndices.has(1)).toBe(true)
        })
    })

    describe('clearRangeState', () => {
        it('resets anchor without clearing selection', () => {
            const state = createSelectionState()
            // Shift-navigate from unselected index 1 to 3 (select mode)
            state.handleShiftNavigation(3, 1, false)
            expect(state.anchorIndex).toBe(1)
            expect(state.selectedIndices.size).toBe(3) // 1, 2, 3

            state.clearRangeState()
            expect(state.anchorIndex).toBeNull()
            // Selection should still contain items
            expect(state.selectedIndices.size).toBe(3)
        })
    })

    describe('edge cases', () => {
        it('handles single-item selection via shift-navigation', () => {
            const state = createSelectionState()
            state.handleShiftNavigation(5, 5, false) // anchor and end are same
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
