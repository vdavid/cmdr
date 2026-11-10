/**
 * Unit tests for `tree-cursor.ts`: what ← and → do from a tree row.
 */

import { describe, expect, it } from 'vitest'
import { nextTreeCursor, treeCursorKeyOf } from './tree-cursor'

function key(name: string, modifiers: Partial<KeyboardEvent> = {}) {
  return { key: name, altKey: false, metaKey: false, ctrlKey: false, shiftKey: false, isComposing: false, ...modifiers }
}

describe('treeCursorKeyOf', () => {
  it('reads bare ← and → and nothing else', () => {
    expect(treeCursorKeyOf(key('ArrowRight'))).toBe('expand')
    expect(treeCursorKeyOf(key('ArrowLeft'))).toBe('collapse')
    expect(treeCursorKeyOf(key('ArrowDown'))).toBeNull()
    expect(treeCursorKeyOf(key('ArrowLeft', { altKey: true }))).toBeNull()
    expect(treeCursorKeyOf(key('ArrowRight', { metaKey: true }))).toBeNull()
    expect(treeCursorKeyOf(key('ArrowRight', { shiftKey: true }))).toBeNull()
    expect(treeCursorKeyOf(key('ArrowRight', { isComposing: true }))).toBeNull()
  })
})

describe('nextTreeCursor', () => {
  const base = { from: 4, count: 20, parentIndex: 1 }

  it('→ opens a shut folder, steps into an open one, and does nothing on a leaf', () => {
    expect(nextTreeCursor('expand', { ...base, row: { level: 2, expanded: false } })).toEqual({ kind: 'expand' })
    expect(nextTreeCursor('expand', { ...base, row: { level: 2, expanded: true } })).toEqual({ kind: 'move', index: 5 })
    expect(nextTreeCursor('expand', { ...base, row: { level: 2 } })).toBeNull()
  })

  it('→ on an open folder in the last row stays put', () => {
    expect(nextTreeCursor('expand', { ...base, from: 19, row: { level: 1, expanded: true } })).toBeNull()
  })

  it('← shuts an open folder, else steps out to the parent', () => {
    expect(nextTreeCursor('collapse', { ...base, row: { level: 2, expanded: true } })).toEqual({ kind: 'collapse' })
    expect(nextTreeCursor('collapse', { ...base, row: { level: 2, expanded: false } })).toEqual({
      kind: 'move',
      index: 1,
    })
    expect(nextTreeCursor('collapse', { ...base, row: { level: 3 } })).toEqual({ kind: 'move', index: 1 })
  })

  it('← on a top-level row that is not open leaves the tree', () => {
    expect(nextTreeCursor('collapse', { ...base, row: { level: 1 } })).toEqual({ kind: 'leave' })
    expect(nextTreeCursor('collapse', { ...base, row: { level: 1, expanded: false } })).toEqual({ kind: 'leave' })
    // An open top-level folder shuts first.
    expect(nextTreeCursor('collapse', { ...base, row: { level: 1, expanded: true } })).toEqual({ kind: 'collapse' })
  })

  it('does nothing on a row still loading, or with a parent the consumer cannot place', () => {
    expect(nextTreeCursor('expand', { ...base, row: undefined })).toBeNull()
    expect(nextTreeCursor('collapse', { ...base, row: undefined })).toBeNull()
    expect(nextTreeCursor('collapse', { ...base, parentIndex: null, row: { level: 2 } })).toBeNull()
  })
})
