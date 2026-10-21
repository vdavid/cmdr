import { describe, it, expect } from 'vitest'
import { listCursorMoveOf, nextListCursor, type ListCursorMove } from './list-cursor'

/** A keydown-like object: the fields the matcher reads, all modifiers explicit. */
function key(name: string, modifiers: Partial<Record<'altKey' | 'metaKey' | 'ctrlKey' | 'shiftKey', boolean>> = {}) {
  return { key: name, altKey: false, metaKey: false, ctrlKey: false, shiftKey: false, isComposing: false, ...modifiers }
}

const down: ListCursorMove = { kind: 'step', delta: 1 }
const up: ListCursorMove = { kind: 'step', delta: -1 }
const pageDown: ListCursorMove = { kind: 'page', delta: 1 }
const pageUp: ListCursorMove = { kind: 'page', delta: -1 }
const first: ListCursorMove = { kind: 'edge', edge: 'first' }
const last: ListCursorMove = { kind: 'edge', edge: 'last' }

describe('listCursorMoveOf', () => {
  it('reads the six bare cursor keys', () => {
    expect(listCursorMoveOf(key('ArrowDown'))).toEqual(down)
    expect(listCursorMoveOf(key('ArrowUp'))).toEqual(up)
    expect(listCursorMoveOf(key('PageDown'))).toEqual(pageDown)
    expect(listCursorMoveOf(key('PageUp'))).toEqual(pageUp)
    expect(listCursorMoveOf(key('Home'))).toEqual(first)
    expect(listCursorMoveOf(key('End'))).toEqual(last)
  })

  it('leaves every modified combo to somebody else', () => {
    for (const modifier of ['altKey', 'metaKey', 'ctrlKey', 'shiftKey'] as const) {
      expect(listCursorMoveOf(key('ArrowDown', { [modifier]: true }))).toBeNull()
      expect(listCursorMoveOf(key('Home', { [modifier]: true }))).toBeNull()
    }
  })

  it('ignores other keys and a key mid-composition', () => {
    expect(listCursorMoveOf(key('ArrowLeft'))).toBeNull()
    expect(listCursorMoveOf(key('Enter'))).toBeNull()
    expect(listCursorMoveOf({ ...key('ArrowDown'), isComposing: true })).toBeNull()
  })

  it('leaves Home and End to a text field when the keys come from one', () => {
    expect(listCursorMoveOf(key('Home'), { textField: true })).toBeNull()
    expect(listCursorMoveOf(key('End'), { textField: true })).toBeNull()
    expect(listCursorMoveOf(key('PageDown'), { textField: true })).toEqual(pageDown)
    expect(listCursorMoveOf(key('ArrowUp'), { textField: true })).toEqual(up)
  })
})

describe('nextListCursor', () => {
  const list = { count: 10, visibleRows: 4 }

  it('steps one row and stops at both ends', () => {
    expect(nextListCursor(down, { ...list, from: 3 })).toBe(4)
    expect(nextListCursor(up, { ...list, from: 3 })).toBe(2)
    expect(nextListCursor(down, { ...list, from: 9 })).toBe(9)
    expect(nextListCursor(up, { ...list, from: 0 })).toBe(0)
  })

  it('wraps a step at the ends when asked, but never a page or an edge', () => {
    const wrapping = { ...list, wrap: true }
    expect(nextListCursor(down, { ...wrapping, from: 9 })).toBe(0)
    expect(nextListCursor(up, { ...wrapping, from: 0 })).toBe(9)
    expect(nextListCursor(pageDown, { ...wrapping, from: 9 })).toBe(9)
    expect(nextListCursor(pageUp, { ...wrapping, from: 0 })).toBe(0)
  })

  it('pages by one row less than a screen, so a row of context stays, and clamps', () => {
    expect(nextListCursor(pageDown, { ...list, from: 0 })).toBe(3)
    expect(nextListCursor(pageUp, { ...list, from: 5 })).toBe(2)
    expect(nextListCursor(pageDown, { ...list, from: 8 })).toBe(9)
    expect(nextListCursor(pageUp, { ...list, from: 1 })).toBe(0)
  })

  it('pages by at least one row however short the view', () => {
    expect(nextListCursor(pageDown, { count: 10, visibleRows: 1, from: 2 })).toBe(3)
    expect(nextListCursor(pageDown, { count: 10, visibleRows: 0, from: 2 })).toBe(3)
  })

  it('goes to either end', () => {
    expect(nextListCursor(first, { ...list, from: 6 })).toBe(0)
    expect(nextListCursor(last, { ...list, from: 2 })).toBe(9)
  })

  it('starts a list with no cursor at the first row, or the last going up', () => {
    expect(nextListCursor(down, { ...list, from: -1 })).toBe(0)
    expect(nextListCursor(pageDown, { ...list, from: -1 })).toBe(0)
    expect(nextListCursor(up, { ...list, from: -1 })).toBe(9)
    expect(nextListCursor(last, { ...list, from: -1 })).toBe(9)
  })

  it('reads a cursor past the end (a list that just shrank) as the last row', () => {
    expect(nextListCursor(up, { ...list, from: 40 })).toBe(8)
    expect(nextListCursor(down, { ...list, from: 40 })).toBe(9)
  })

  it('answers -1 for an empty list', () => {
    for (const move of [down, up, pageDown, pageUp, first, last]) {
      expect(nextListCursor(move, { count: 0, visibleRows: 4, from: -1 })).toBe(-1)
    }
  })

  describe('with group headings', () => {
    // Headings at 0, 3, and 9: rows 1–2 and 4–8 take the cursor.
    const headings = new Set([0, 3, 9])
    const grouped = { count: 10, visibleRows: 4, isSkipped: (index: number) => headings.has(index) }

    it('steps over a heading', () => {
      expect(nextListCursor(down, { ...grouped, from: 2 })).toBe(4)
      expect(nextListCursor(up, { ...grouped, from: 4 })).toBe(2)
    })

    it('stays on the first and last rows rather than landing on a heading', () => {
      expect(nextListCursor(up, { ...grouped, from: 1 })).toBe(1)
      expect(nextListCursor(down, { ...grouped, from: 8 })).toBe(8)
      expect(nextListCursor(first, { ...grouped, from: 6 })).toBe(1)
      expect(nextListCursor(last, { ...grouped, from: 2 })).toBe(8)
    })

    it('wraps past a heading at either end', () => {
      expect(nextListCursor(down, { ...grouped, wrap: true, from: 8 })).toBe(1)
      expect(nextListCursor(up, { ...grouped, wrap: true, from: 1 })).toBe(8)
    })

    it('carries a page that lands on a heading on to the next row, or back when none is left', () => {
      // 1 + 3 = 4 is a row; 0 + 3 = 3 is a heading, so on to 4.
      expect(nextListCursor(pageDown, { ...grouped, from: 1 })).toBe(4)
      expect(nextListCursor(pageDown, { ...grouped, from: 0 })).toBe(4)
      // 7 + 3 = 10, clamped to the heading at 9, with nothing after it: back to 8.
      expect(nextListCursor(pageDown, { ...grouped, from: 7 })).toBe(8)
      // 6 - 3 = 3, a heading: on up to 2.
      expect(nextListCursor(pageUp, { ...grouped, from: 6 })).toBe(2)
    })

    it('answers -1 when every row is a heading', () => {
      expect(nextListCursor(down, { count: 2, visibleRows: 4, from: -1, isSkipped: () => true })).toBe(-1)
    })
  })
})
