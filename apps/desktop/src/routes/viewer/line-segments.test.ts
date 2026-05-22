import { describe, it, expect } from 'vitest'

import { segmentLine } from './line-segments'

describe('segmentLine', () => {
  it('no matches and no selection returns one unmarked span', () => {
    const segs = segmentLine('hello world', [], null)
    expect(segs).toEqual([{ text: 'hello world', highlight: false, active: false, selected: false }])
  })

  it('empty line returns a single empty span when nothing is highlighted', () => {
    const segs = segmentLine('', [], null)
    expect(segs).toEqual([{ text: '', highlight: false, active: false, selected: false }])
  })

  it('single search match splits into three segments', () => {
    const segs = segmentLine('hello world', [{ column: 6, length: 5, active: false }], null)
    expect(segs).toEqual([
      { text: 'hello ', highlight: false, active: false, selected: false },
      { text: 'world', highlight: true, active: false, selected: false },
    ])
  })

  it('active match flag propagates to its segment', () => {
    const segs = segmentLine('hello world', [{ column: 0, length: 5, active: true }], null)
    expect(segs).toEqual([
      { text: 'hello', highlight: true, active: true, selected: false },
      { text: ' world', highlight: false, active: false, selected: false },
    ])
  })

  it('two search matches with text between them', () => {
    const segs = segmentLine(
      'foo bar baz',
      [
        { column: 0, length: 3, active: false },
        { column: 8, length: 3, active: false },
      ],
      null,
    )
    expect(segs).toEqual([
      { text: 'foo', highlight: true, active: false, selected: false },
      { text: ' bar ', highlight: false, active: false, selected: false },
      { text: 'baz', highlight: true, active: false, selected: false },
    ])
  })

  it('selection only (no search) splits into selected and unselected spans', () => {
    const segs = segmentLine('hello world', [], { selStart: 6, selEnd: 11 })
    expect(segs).toEqual([
      { text: 'hello ', highlight: false, active: false, selected: false },
      { text: 'world', highlight: false, active: false, selected: true },
    ])
  })

  it('selection covering the whole line returns one selected segment', () => {
    const segs = segmentLine('abc', [], { selStart: 0, selEnd: 3 })
    expect(segs).toEqual([{ text: 'abc', highlight: false, active: false, selected: true }])
  })

  it('selection and search overlap: span is both highlighted and selected', () => {
    // "hello world" — search match on "lo w" (cols 3..7), selection on "world" (6..11).
    // Overlap: "lo " (3..6) is search only, " w" (6..7) is search+select, "orld" (7..11) is select only.
    const segs = segmentLine('hello world', [{ column: 3, length: 4, active: false }], {
      selStart: 6,
      selEnd: 11,
    })
    expect(segs).toEqual([
      { text: 'hel', highlight: false, active: false, selected: false },
      { text: 'lo ', highlight: true, active: false, selected: false },
      { text: 'w', highlight: true, active: false, selected: true },
      { text: 'orld', highlight: false, active: false, selected: true },
    ])
  })

  it('selection entirely inside a search match: produces three segments (h, h+s, h)', () => {
    // "hello world" — match "hello world" (0..11), select "lo w" (3..7).
    const segs = segmentLine('hello world', [{ column: 0, length: 11, active: false }], {
      selStart: 3,
      selEnd: 7,
    })
    expect(segs).toEqual([
      { text: 'hel', highlight: true, active: false, selected: false },
      { text: 'lo w', highlight: true, active: false, selected: true },
      { text: 'orld', highlight: true, active: false, selected: false },
    ])
  })

  it('clamps out-of-range match columns to the line length', () => {
    // Match advertises column 20 but line is only 11 chars; we still don't crash and the segment is empty.
    const segs = segmentLine('hello world', [{ column: 20, length: 5, active: false }], null)
    expect(segs).toEqual([{ text: 'hello world', highlight: false, active: false, selected: false }])
  })

  it('selection at the line end (selEnd == lineLength) selects through the final char', () => {
    const segs = segmentLine('abc', [], { selStart: 1, selEnd: 3 })
    expect(segs).toEqual([
      { text: 'a', highlight: false, active: false, selected: false },
      { text: 'bc', highlight: false, active: false, selected: true },
    ])
  })

  it('UTF-16 surrogate pair: offsets are in code units', () => {
    // "👋hi" = 4 UTF-16 units (2 for emoji + h + i).
    // Select just the emoji: 0..2.
    const segs = segmentLine('👋hi', [], { selStart: 0, selEnd: 2 })
    expect(segs).toEqual([
      { text: '👋', highlight: false, active: false, selected: true },
      { text: 'hi', highlight: false, active: false, selected: false },
    ])
  })
})
