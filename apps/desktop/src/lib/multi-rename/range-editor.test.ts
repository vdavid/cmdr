import { describe, it, expect } from 'vitest'
import {
  boxNeedsSync,
  boxOffsetsFor,
  isWholeField,
  rangeReading,
  selectionInText,
  selectionPieces,
  textOffsetsOf,
  textPick,
} from './range-editor'

describe('a selection in the text box', () => {
  // `Beach day`: nine characters, `B` at 0 and `y` at 8.
  const pick = (start: number, end: number, form: 'span' | 'length' = 'span') =>
    textPick({ text: 'Beach day', start, end, form }).range

  it('takes the whole field when it’s only a caret, wherever the caret is', () => {
    expect(selectionInText('Beach day', 3, 3)).toBeNull()
    expect(textPick({ text: 'Beach day', start: 3, end: 3, form: 'span' })).toEqual({
      selection: null,
      range: { kind: 'all' },
    })
    expect(pick(0, 0)).toEqual({ kind: 'all' })
    expect(pick(9, 9)).toEqual({ kind: 'all' })
  })

  it('takes the whole field on an empty field, the length form too', () => {
    expect(selectionInText('', 0, 0)).toBeNull()
    expect(textPick({ text: '', start: 0, end: 0, form: 'length' })).toEqual({
      selection: null,
      range: { kind: 'all' },
    })
  })

  it('hands back the characters it picked with the range', () => {
    expect(textPick({ text: 'Beach day', start: 1, end: 4, form: 'span' })).toEqual({
      selection: { first: 1, last: 3, length: 9 },
      range: { kind: 'span', at: 2, to: 4 },
    })
  })

  it('keeps the token’s own edges when asked, as a key only adjusting the selection does', () => {
    const keep = { startFromEnd: false, endFromEnd: false }
    expect(textPick({ text: 'Beach day', start: 6, end: 9, form: 'span', keep }).range).toEqual({
      kind: 'span',
      at: 7,
      to: 9,
    })
    expect(textPick({ text: 'Beach day', start: 0, end: 9, form: 'span', keep }).range).toEqual({
      kind: 'span',
      at: 1,
      to: 9,
    })
  })

  it('reads the whole field as the bare field', () => {
    expect(selectionInText('Beach day', 0, 9)).toEqual({ first: 0, last: 8, length: 9 })
    expect(pick(0, 9)).toEqual({ kind: 'all' })
  })

  it('reads one character as one', () => {
    expect(pick(2, 3)).toEqual({ kind: 'one', at: 3 })
  })

  it('counts each end from the edge it’s nearer, so a run near the end counts from the end', () => {
    expect(pick(0, 5)).toEqual({ kind: 'span', at: 1, to: 5 })
    expect(pick(6, 8)).toEqual({ kind: 'span', at: -3, to: 2 })
    expect(pick(6, 9)).toEqual({ kind: 'from', at: -3 })
  })

  it('keeps the length form', () => {
    expect(pick(1, 5, 'length')).toEqual({ kind: 'length', at: 2, length: 4 })
  })

  it('counts characters as the engine does, not UTF-16 units', () => {
    // `a😀b`: the emoji is one character but two UTF-16 units, so `b` sits at units 3–4.
    expect(selectionInText('a😀b', 1, 3)).toEqual({ first: 1, last: 1, length: 3 })
    expect(selectionInText('a😀b', 3, 4)).toEqual({ first: 2, last: 2, length: 3 })
  })
})

describe('textOffsetsOf', () => {
  it('puts the engine’s bounds back as the text box’s selection', () => {
    expect(textOffsetsOf('Beach day', { first: 0, last: 4, length: 9 })).toEqual({ start: 0, end: 5 })
    expect(textOffsetsOf('Beach day', { first: 8, last: 8, length: 9 })).toEqual({ start: 8, end: 9 })
  })

  it('counts UTF-16 units past a character that takes two', () => {
    expect(textOffsetsOf('a😀b', { first: 1, last: 2, length: 3 })).toEqual({ start: 1, end: 4 })
  })

  it('round-trips with selectionInText', () => {
    const text = 'x😀 yz'
    const selection = { first: 1, last: 3, length: 5 }
    const { start, end } = textOffsetsOf(text, selection)
    expect(selectionInText(text, start, end)).toEqual(selection)
  })
})

describe('isWholeField', () => {
  it('knows the bare field and its from-the-1st spelling', () => {
    expect(isWholeField({ kind: 'all' })).toBe(true)
    expect(isWholeField({ kind: 'from', at: 1 })).toBe(true)
  })

  it('knows every other range takes a part', () => {
    expect(isWholeField({ kind: 'from', at: 2 })).toBe(false)
    expect(isWholeField({ kind: 'span', at: 1, to: -1 })).toBe(false)
    expect(isWholeField({ kind: 'one', at: 1 })).toBe(false)
    expect(isWholeField({ kind: 'length', at: 1, length: 9 })).toBe(false)
  })
})

describe('the box’s selection set from outside', () => {
  it('selects the engine’s bounds', () => {
    expect(boxOffsetsFor('Beach day', { first: 0, last: 4, length: 9 })).toEqual({ start: 0, end: 5 })
  })

  it('selects the whole text for the whole field', () => {
    expect(boxOffsetsFor('Beach day', 'whole')).toEqual({ start: 0, end: 9 })
    expect(boxOffsetsFor('a😀', 'whole')).toEqual({ start: 0, end: 3 })
    expect(boxOffsetsFor('', 'whole')).toEqual({ start: 0, end: 0 })
  })

  it('puts a caret at the end when the token takes nothing from the file', () => {
    expect(boxOffsetsFor('Beach day', null)).toEqual({ start: 9, end: 9 })
    expect(boxOffsetsFor('a😀', null)).toEqual({ start: 3, end: 3 })
    expect(boxOffsetsFor('', null)).toEqual({ start: 0, end: 0 })
  })
})

describe('selectionPieces', () => {
  it('splits the box’s text at its selection, in the field’s UTF-16 offsets', () => {
    expect(selectionPieces('Beach day', 0, 5)).toEqual({ before: '', selected: 'Beach', after: ' day' })
    expect(selectionPieces('a😀b', 1, 3)).toEqual({ before: 'a', selected: '😀', after: 'b' })
  })

  it('selects nothing for a caret, and clamps offsets past the end', () => {
    expect(selectionPieces('Beach day', 4, 4)).toEqual({ before: 'Beac', selected: '', after: 'h day' })
    expect(selectionPieces('jpg', 1, 9)).toEqual({ before: 'j', selected: 'pg', after: '' })
  })
})

describe('boxNeedsSync', () => {
  it('syncs a token the box didn’t write', () => {
    expect(boxNeedsSync({ token: 'N1-5', text: 'Beach day', wrote: null })).toBe(true)
    expect(boxNeedsSync({ token: 'N1-5', text: 'Beach day', wrote: { token: 'N1-3', text: 'Beach day' } })).toBe(true)
  })

  it('leaves the box alone for the token it wrote itself, so a caret is never re-selected', () => {
    expect(boxNeedsSync({ token: 'N', text: 'Beach day', wrote: { token: 'N', text: 'Beach day' } })).toBe(false)
  })

  it('syncs the token the box wrote once the cursor’s file changes under it', () => {
    expect(boxNeedsSync({ token: 'N1-5', text: 'IMG_2041', wrote: { token: 'N1-5', text: 'Beach day' } })).toBe(true)
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
