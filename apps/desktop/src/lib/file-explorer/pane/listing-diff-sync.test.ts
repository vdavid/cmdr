/**
 * Pure-logic tests for `reconcileCursorAndSelection`, the cursor + selection
 * reconciliation core of the file-watcher `directory-diff` handler. These pin
 * the off-by-one `..`-row offset bookkeeping that the integration suite
 * (`selection-consistency.test.ts`) exercises only indirectly via a mounted pane.
 */
import { describe, it, expect } from 'vitest'
import { reconcileCursorAndSelection } from './listing-diff-sync.svelte'
import type { DiffChange, FileEntry } from '../types'

function entry(name: string): FileEntry {
  return {
    name,
    path: `/test/${name}`,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
  }
}

function remove(index: number, name = `f${String(index)}`): DiffChange {
  return { type: 'remove', entry: entry(name), index }
}

function add(index: number, name = `f${String(index)}`): DiffChange {
  return { type: 'add', entry: entry(name), index }
}

function modify(index: number, name = `f${String(index)}`): DiffChange {
  return { type: 'modify', entry: entry(name), index }
}

describe('reconcileCursorAndSelection', () => {
  it('leaves cursor and selection untouched when the diff has no structural changes', () => {
    const result = reconcileCursorAndSelection({
      changes: [modify(2)],
      hasParent: false,
      cursorIndex: 3,
      selectedIndices: [1, 3],
      operationSelectedNames: null,
      count: 10,
    })
    expect(result).toEqual({ cursorIndex: 3, selectedIndices: null })
  })

  it('shifts the cursor down by the number of removals before it (no parent row)', () => {
    // Backend indices 0 and 1 removed; cursor was at backend index 4 -> 2.
    const result = reconcileCursorAndSelection({
      changes: [remove(0), remove(1)],
      hasParent: false,
      cursorIndex: 4,
      selectedIndices: [],
      operationSelectedNames: null,
      count: 8,
    })
    expect(result.cursorIndex).toBe(2)
    expect(result.selectedIndices).toBeNull()
  })

  it('applies the +1 parent-row offset to the cursor on both sides of the shift', () => {
    // With a `..` row, frontend cursor 5 == backend 4. Remove backend 0 and 1,
    // backend 4 -> 2, frontend 2 + 1 == 3.
    const result = reconcileCursorAndSelection({
      changes: [remove(0), remove(1)],
      hasParent: true,
      cursorIndex: 5,
      selectedIndices: [],
      operationSelectedNames: null,
      count: 8,
    })
    expect(result.cursorIndex).toBe(3)
  })

  it('clamps the cursor into range when its own backend row was removed', () => {
    // Cursor sat on a removed row -> adjustSelectionIndices drops it, so we clamp
    // to count - 1 (no parent row).
    const result = reconcileCursorAndSelection({
      changes: [remove(3)],
      hasParent: false,
      cursorIndex: 3,
      selectedIndices: [],
      operationSelectedNames: null,
      count: 5,
    })
    expect(result.cursorIndex).toBe(3)
  })

  it('reindexes the selection in backend space and re-applies the parent offset', () => {
    // hasParent: frontend selection [2, 4] == backend [1, 3]. Remove backend 0:
    // backend [1, 3] -> [0, 2], frontend -> [1, 3].
    const result = reconcileCursorAndSelection({
      changes: [remove(0)],
      hasParent: true,
      cursorIndex: 1,
      selectedIndices: [2, 4],
      operationSelectedNames: null,
      count: 6,
    })
    expect(result.selectedIndices).toEqual([1, 3])
  })

  it('leaves the selection untouched while an operation owns it (operationSelectedNames set)', () => {
    const result = reconcileCursorAndSelection({
      changes: [remove(0)],
      hasParent: false,
      cursorIndex: 2,
      selectedIndices: [1, 2],
      operationSelectedNames: ['f1.txt'],
      count: 5,
    })
    // Cursor still adjusts, but the selection is left for the operation's own
    // name-based re-resolution to handle.
    expect(result.selectedIndices).toBeNull()
  })

  it('leaves an empty selection as null rather than an empty array', () => {
    const result = reconcileCursorAndSelection({
      changes: [add(0)],
      hasParent: false,
      cursorIndex: 0,
      selectedIndices: [],
      operationSelectedNames: null,
      count: 5,
    })
    expect(result.selectedIndices).toBeNull()
  })

  it('shifts the selection up to account for insertions before it', () => {
    // No parent. Selection backend [2, 3]; add at backend 0 and 1 -> [4, 5].
    const result = reconcileCursorAndSelection({
      changes: [add(0), add(1)],
      hasParent: false,
      cursorIndex: 2,
      selectedIndices: [2, 3],
      operationSelectedNames: null,
      count: 7,
    })
    expect(result.selectedIndices).toEqual([4, 5])
  })
})
