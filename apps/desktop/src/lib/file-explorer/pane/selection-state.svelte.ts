import { SvelteSet } from 'svelte/reactivity'

// SAFETY CONTRACT: selectedIndices is the single source of truth for what files are selected.
// Both the UI (via props) and file operations (via getSelectedIndices()) read from this same Set.
// CRITICAL: Always use mutations (.add(), .delete(), .clear()) - never reassign.
// SvelteSet only tracks mutations for reactivity.

export function createSelectionState(options?: { onChanged?: () => void }) {
    const onChanged = options?.onChanged

    const selectedIndices: SvelteSet<number> = new SvelteSet()
    let selectionAnchorIndex = $state<number | null>(null)
    let selectionEndIndex = $state<number | null>(null)
    let isDeselecting = $state(false)

    // Get indices in range [a, b] inclusive, skipping ".." entry (index 0 when hasParent)
    function getIndicesInRange(a: number, b: number, hasParent: boolean): number[] {
        const start = Math.min(a, b)
        const end = Math.max(a, b)
        const indices: number[] = []
        for (let i = start; i <= end; i++) {
            // Skip ".." entry
            if (hasParent && i === 0) continue
            indices.push(i)
        }
        return indices
    }

    // Apply range selection from anchor to end.
    // Handles both selection and deselection modes, including range shrinking.
    // When cursor returns to anchor (newEnd === anchor), nothing is selected.
    function applyRangeSelection(newEnd: number, hasParent: boolean) {
        if (selectionAnchorIndex === null) return

        // When cursor returns to anchor, range is empty (nothing selected)
        const rangeIsEmpty = newEnd === selectionAnchorIndex
        const newRange = rangeIsEmpty ? [] : getIndicesInRange(selectionAnchorIndex, newEnd, hasParent)

        if (isDeselecting) {
            // Deselection mode: remove items in range
            for (const i of newRange) {
                selectedIndices.delete(i)
            }
        } else {
            // Selection mode: add items in range
            for (const i of newRange) {
                selectedIndices.add(i)
            }
        }

        // Handle range shrinking: if old range was larger, clear the difference
        if (selectionEndIndex !== null) {
            const oldRange =
                selectionEndIndex === selectionAnchorIndex
                    ? []
                    : getIndicesInRange(selectionAnchorIndex, selectionEndIndex, hasParent)
            for (const i of oldRange) {
                if (!newRange.includes(i)) {
                    if (isDeselecting) {
                        // In deselect mode, shrinking means we stop deselecting those items.
                        // They stay in whatever state they were before this selection action.
                        // Since we track from start, we need to re-add them if they were selected.
                        // For simplicity, in deselect mode we just keep them deselected.
                    } else {
                        // In select mode, shrinking means we deselect the items no longer in range
                        selectedIndices.delete(i)
                    }
                }
            }
        }

        selectionEndIndex = newEnd
    }

    function clearRangeState() {
        selectionAnchorIndex = null
        selectionEndIndex = null
        isDeselecting = false
    }

    function clearSelection() {
        selectedIndices.clear()
        selectionAnchorIndex = null
        selectionEndIndex = null
        isDeselecting = false
        onChanged?.()
    }

    function toggleAt(index: number, hasParent: boolean): boolean {
        // Can't select ".." entry
        if (hasParent && index === 0) return false

        if (selectedIndices.has(index)) {
            selectedIndices.delete(index)
            return false
        } else {
            selectedIndices.add(index)
            return true
        }
    }

    function handleShiftNavigation(newIndex: number, cursorIndex: number, hasParent: boolean) {
        // Set anchor if not already set (use current cursor position before moving)
        if (selectionAnchorIndex === null) {
            selectionAnchorIndex = cursorIndex
            // Determine if we're in deselect mode (anchor was already selected)
            isDeselecting = selectedIndices.has(cursorIndex)
        }

        // Apply the range selection
        applyRangeSelection(newIndex, hasParent)
    }

    function selectAll(hasParent: boolean, effectiveTotalCount: number) {
        selectedIndices.clear()
        const startIndex = hasParent ? 1 : 0 // Skip ".." entry
        for (let i = startIndex; i < effectiveTotalCount; i++) {
            selectedIndices.add(i)
        }
        clearRangeState()
        onChanged?.()
    }

    function deselectAll() {
        selectedIndices.clear()
        clearRangeState()
        onChanged?.()
    }

    function selectRange(startIndex: number, endIndex: number, hasParent: boolean) {
        const indices = getIndicesInRange(startIndex, endIndex, hasParent)
        for (const i of indices) {
            selectedIndices.add(i)
        }
        clearRangeState()
    }

    function isAllSelected(hasParent: boolean, effectiveTotalCount: number): boolean {
        const selectableCount = hasParent ? effectiveTotalCount - 1 : effectiveTotalCount
        return selectedIndices.size === selectableCount && selectableCount > 0
    }

    function getSelectedIndices(): number[] {
        return Array.from(selectedIndices)
    }

    function setSelectedIndices(indices: number[]) {
        selectedIndices.clear()
        for (const i of indices) {
            selectedIndices.add(i)
        }
        clearRangeState()
        onChanged?.()
    }

    return {
        /** The selected indices set. Use .size, .has(), iterate -- but don't reassign. */
        get selectedIndices() {
            return selectedIndices
        },
        get anchorIndex() {
            return selectionAnchorIndex
        },

        clearSelection,
        toggleAt,
        handleShiftNavigation,
        clearRangeState,
        selectAll,
        deselectAll,
        selectRange,
        isAllSelected,
        getSelectedIndices,
        setSelectedIndices,
    }
}
