import { describe, it, expect } from 'vitest'
import { extendAnchor, moveSelection, rangeReading, ribbonMoveOf, spanBetween } from './range-editor'

const nine = (first: number, last: number) => ({ first, last, length: 9 })

describe('moveSelection', () => {
  it('moves the whole selection, and stops at either end', () => {
    expect(moveSelection(nine(1, 3), { step: 1, edge: 'both' })).toEqual(nine(2, 4))
    expect(moveSelection(nine(0, 3), { step: -1, edge: 'both' })).toEqual(nine(0, 3))
    expect(moveSelection(nine(6, 8), { step: 1, edge: 'both' })).toEqual(nine(6, 8))
  })

  it('moves the end alone, never past the start or the last character', () => {
    expect(moveSelection(nine(1, 3), { step: 1, edge: 'end' })).toEqual(nine(1, 4))
    expect(moveSelection(nine(1, 1), { step: -1, edge: 'end' })).toEqual(nine(1, 1))
    expect(moveSelection(nine(1, 8), { step: 1, edge: 'end' })).toEqual(nine(1, 8))
  })

  it('moves the start alone, never past the end or the first character', () => {
    expect(moveSelection(nine(1, 3), { step: -1, edge: 'start' })).toEqual(nine(0, 3))
    expect(moveSelection(nine(3, 3), { step: 1, edge: 'start' })).toEqual(nine(3, 3))
    expect(moveSelection(nine(0, 3), { step: -1, edge: 'start' })).toEqual(nine(0, 3))
  })
})

describe('a drag', () => {
  it('spans its two characters either way round', () => {
    expect(spanBetween(5, 2, 9)).toEqual(nine(2, 5))
    expect(spanBetween(2, 2, 9)).toEqual(nine(2, 2))
  })

  it('extends from the far edge on ⇧-click', () => {
    expect(extendAnchor(nine(2, 4), 7)).toBe(2)
    expect(extendAnchor(nine(2, 4), 0)).toBe(4)
  })
})

describe('rangeReading', () => {
  it('reads each form in plain words', () => {
    expect(rangeReading({ kind: 'all' })).toEqual({ key: 'multiRename.rangeEditor.reading.whole', params: {} })
    expect(rangeReading({ kind: 'from', at: 1 })).toEqual({ key: 'multiRename.rangeEditor.reading.whole', params: {} })
    expect(rangeReading({ kind: 'one', at: 3 })).toEqual({
      key: 'multiRename.rangeEditor.reading.one',
      params: { at: 3 },
    })
    expect(rangeReading({ kind: 'one', at: -1 })).toEqual({
      key: 'multiRename.rangeEditor.reading.oneFromEnd',
      params: { at: 1 },
    })
    expect(rangeReading({ kind: 'from', at: 2 })).toEqual({
      key: 'multiRename.rangeEditor.reading.toEnd',
      params: { at: 2 },
    })
    expect(rangeReading({ kind: 'from', at: -3 })).toEqual({
      key: 'multiRename.rangeEditor.reading.lastChars',
      params: { count: 3 },
    })
    expect(rangeReading({ kind: 'span', at: 2, to: 5 })).toEqual({
      key: 'multiRename.rangeEditor.reading.span',
      params: { from: 2, to: 5 },
    })
    expect(rangeReading({ kind: 'length', at: 7, length: 3 })).toEqual({
      key: 'multiRename.rangeEditor.reading.length',
      params: { count: 3, at: 7 },
    })
    expect(rangeReading({ kind: 'length', at: -3, length: 3 })).toEqual({
      key: 'multiRename.rangeEditor.reading.lengthFromEnd',
      params: { count: 3, at: 3 },
    })
  })

  it('counts an end from the end when the token does, a negative start taking a positive end with it', () => {
    expect(rangeReading({ kind: 'span', at: 1, to: -5 })).toEqual({
      key: 'multiRename.rangeEditor.reading.spanToFromEnd',
      params: { from: 1, to: 5 },
    })
    expect(rangeReading({ kind: 'span', at: -8, to: 5 })).toEqual({
      key: 'multiRename.rangeEditor.reading.spanFromEnd',
      params: { from: 8, to: 5 },
    })
    expect(rangeReading({ kind: 'span', at: -8, to: -5 })).toEqual({
      key: 'multiRename.rangeEditor.reading.spanFromEnd',
      params: { from: 8, to: 5 },
    })
  })
})

describe('ribbonMoveOf', () => {
  const press = (key: string, mods: { shiftKey?: boolean; altKey?: boolean; metaKey?: boolean } = {}) =>
    ribbonMoveOf({ key, shiftKey: false, altKey: false, metaKey: false, ctrlKey: false, ...mods })

  it('moves the selection with ← / →, its end with ⇧, its start with ⌥', () => {
    expect(press('ArrowRight')).toEqual({ step: 1, edge: 'both' })
    expect(press('ArrowLeft', { shiftKey: true })).toEqual({ step: -1, edge: 'end' })
    expect(press('ArrowRight', { altKey: true })).toEqual({ step: 1, edge: 'start' })
  })

  it('leaves every other key and combination alone', () => {
    expect(press('ArrowUp')).toBeNull()
    expect(press('ArrowRight', { metaKey: true })).toBeNull()
    expect(press('ArrowRight', { shiftKey: true, altKey: true })).toBeNull()
  })
})
