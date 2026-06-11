import { describe, it, expect, vi } from 'vitest'
import { firstSelectedIndex } from './first-selected-index'
import { createSelectionState } from './selection-state.svelte'

describe('firstSelectedIndex', () => {
  it('returns the lowest index when there is no parent row', () => {
    expect(firstSelectedIndex([2, 5, 7], false)).toBe(2)
  })

  it('never lands on the `..` row: skips a leading index 0 when hasParent', () => {
    // A `*`-style match can include the synthetic `..` at 0; the cursor must skip it.
    expect(firstSelectedIndex([0, 3, 4], true)).toBe(3)
  })

  it('keeps index 0 when there is NO parent row (0 is a real file)', () => {
    expect(firstSelectedIndex([0, 3, 4], false)).toBe(0)
  })

  it('returns the only selectable index past the parent row', () => {
    expect(firstSelectedIndex([0, 9], true)).toBe(9)
  })

  it('returns null for an empty set', () => {
    expect(firstSelectedIndex([], true)).toBeNull()
    expect(firstSelectedIndex([], false)).toBeNull()
  })

  it('returns null when the only entry is the `..` row under hasParent', () => {
    expect(firstSelectedIndex([0], true)).toBeNull()
  })
})

/**
 * Pins `FilePane.applyIndices`'s post-select cursor jump without mounting the component
 * (same approach as `has-parent.test.ts` pairing the pure helper with real `selectAll`).
 * Replicates the method body: apply to the real selection state, then move the cursor on
 * `add` only. `setCursorIndex` is the pane's cursor-move + scroll-into-view primitive, so a
 * call on it IS the scroll-into-view request.
 */
function runApplyIndices(
  idxs: number[],
  mode: 'add' | 'remove',
  hasParent: boolean,
  setCursorIndex: (index: number) => void,
): void {
  const selection = createSelectionState()
  selection.applyIndices(idxs, mode, hasParent)
  if (mode === 'add') {
    const target = firstSelectedIndex(idxs, hasParent)
    if (target !== null) setCursorIndex(target)
  }
}

describe('applyIndices post-select cursor jump (integration)', () => {
  it('moves the cursor to the first selected file on add, scrolling it into view', () => {
    const setCursorIndex = vi.fn()
    runApplyIndices([2, 5, 7], 'add', false, setCursorIndex)
    expect(setCursorIndex).toHaveBeenCalledTimes(1)
    expect(setCursorIndex).toHaveBeenCalledWith(2)
  })

  it('lands on the first real file, never the `..` row, when hasParent', () => {
    const setCursorIndex = vi.fn()
    // The match included the synthetic `..` at index 0; the cursor must skip it.
    runApplyIndices([0, 4, 8], 'add', true, setCursorIndex)
    expect(setCursorIndex).toHaveBeenCalledWith(4)
  })

  it('does NOT move the cursor on remove (deselect)', () => {
    const setCursorIndex = vi.fn()
    runApplyIndices([2, 5, 7], 'remove', false, setCursorIndex)
    expect(setCursorIndex).not.toHaveBeenCalled()
  })

  it('does not move the cursor when an add selects nothing selectable', () => {
    const setCursorIndex = vi.fn()
    runApplyIndices([0], 'add', true, setCursorIndex) // only `..`, which is skipped
    expect(setCursorIndex).not.toHaveBeenCalled()
  })
})
