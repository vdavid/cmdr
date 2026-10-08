import { describe, it, expect } from 'vitest'

import {
  type AnnouncedRow,
  compareRowOffset,
  describeSelectionForAt,
  EOF_ROW,
  estimateSelectionBytes,
  extendSelection,
  getRowSegmentBounds,
  isEmpty,
  isRowInRange,
  selectionBytesFromFileSize,
  rowOffsetEquals,
  makeSelectAll,
  makeSelectToEof,
  MAX_ANNOUNCE_ROWS,
  normaliseSelection,
  toRangeEnds,
  type Selection,
} from './selection.svelte'

describe('compareRowOffset', () => {
  it('returns 0 for identical points', () => {
    expect(compareRowOffset({ row: 3, offset: 5 }, { row: 3, offset: 5 })).toBe(0)
  })

  it('compares by line first', () => {
    expect(compareRowOffset({ row: 2, offset: 99 }, { row: 3, offset: 0 })).toBeLessThan(0)
    expect(compareRowOffset({ row: 5, offset: 0 }, { row: 2, offset: 99 })).toBeGreaterThan(0)
  })

  it('compares by offset when lines match', () => {
    expect(compareRowOffset({ row: 4, offset: 1 }, { row: 4, offset: 7 })).toBeLessThan(0)
    expect(compareRowOffset({ row: 4, offset: 7 }, { row: 4, offset: 1 })).toBeGreaterThan(0)
  })
})

describe('rowOffsetEquals', () => {
  it('returns true for identical points', () => {
    expect(rowOffsetEquals({ row: 0, offset: 0 }, { row: 0, offset: 0 })).toBe(true)
  })
  it('returns false when lines differ', () => {
    expect(rowOffsetEquals({ row: 0, offset: 5 }, { row: 1, offset: 5 })).toBe(false)
  })
  it('returns false when offsets differ', () => {
    expect(rowOffsetEquals({ row: 7, offset: 1 }, { row: 7, offset: 2 })).toBe(false)
  })
})

describe('normaliseSelection', () => {
  it('returns endpoints unchanged when already in order', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 3, offset: 4 } }
    const { start, end } = normaliseSelection(sel)
    expect(start).toEqual({ row: 0, offset: 0 })
    expect(end).toEqual({ row: 3, offset: 4 })
  })

  it('swaps endpoints when reversed (anchor below focus)', () => {
    const sel: Selection = { anchor: { row: 5, offset: 2 }, focus: { row: 1, offset: 8 } }
    const { start, end } = normaliseSelection(sel)
    expect(start).toEqual({ row: 1, offset: 8 })
    expect(end).toEqual({ row: 5, offset: 2 })
  })

  it('handles same-line reversed selection', () => {
    const sel: Selection = { anchor: { row: 2, offset: 10 }, focus: { row: 2, offset: 3 } }
    const { start, end } = normaliseSelection(sel)
    expect(start).toEqual({ row: 2, offset: 3 })
    expect(end).toEqual({ row: 2, offset: 10 })
  })
})

describe('isEmpty', () => {
  it('null is empty', () => {
    expect(isEmpty(null)).toBe(true)
  })

  it('anchor == focus is empty (caret-only click)', () => {
    expect(isEmpty({ anchor: { row: 2, offset: 5 }, focus: { row: 2, offset: 5 } })).toBe(true)
  })

  it('different endpoints means not empty', () => {
    expect(isEmpty({ anchor: { row: 0, offset: 0 }, focus: { row: 0, offset: 1 } })).toBe(false)
  })
})

describe('isRowInRange', () => {
  const sel: Selection = { anchor: { row: 2, offset: 3 }, focus: { row: 5, offset: 7 } }

  it('returns false for lines before the start', () => {
    expect(isRowInRange(sel, 0)).toBe(false)
    expect(isRowInRange(sel, 1)).toBe(false)
  })

  it('returns true for start line, end line, and intermediate lines', () => {
    expect(isRowInRange(sel, 2)).toBe(true)
    expect(isRowInRange(sel, 3)).toBe(true)
    expect(isRowInRange(sel, 4)).toBe(true)
    expect(isRowInRange(sel, 5)).toBe(true)
  })

  it('returns false for lines after the end', () => {
    expect(isRowInRange(sel, 6)).toBe(false)
    expect(isRowInRange(sel, 100)).toBe(false)
  })

  it('returns false for empty selections', () => {
    expect(isRowInRange(null, 5)).toBe(false)
    expect(isRowInRange({ anchor: { row: 5, offset: 0 }, focus: { row: 5, offset: 0 } }, 5)).toBe(false)
  })

  it('works with reversed selections (anchor below focus)', () => {
    const reversed: Selection = { anchor: { row: 5, offset: 0 }, focus: { row: 2, offset: 0 } }
    expect(isRowInRange(reversed, 3)).toBe(true)
    expect(isRowInRange(reversed, 1)).toBe(false)
  })
})

describe('getRowSegmentBounds', () => {
  it('returns null for empty selections', () => {
    expect(getRowSegmentBounds(null, 0, 10)).toBeNull()
  })

  it('returns null for lines outside the range', () => {
    const sel: Selection = { anchor: { row: 2, offset: 0 }, focus: { row: 4, offset: 5 } }
    expect(getRowSegmentBounds(sel, 1, 10)).toBeNull()
    expect(getRowSegmentBounds(sel, 5, 10)).toBeNull()
  })

  it('single-line selection: bounds are start.offset .. end.offset', () => {
    const sel: Selection = { anchor: { row: 3, offset: 2 }, focus: { row: 3, offset: 7 } }
    expect(getRowSegmentBounds(sel, 3, 20)).toEqual({ selStart: 2, selEnd: 7 })
  })

  it('start line of multi-line: bounds are start.offset .. lineLength', () => {
    const sel: Selection = { anchor: { row: 2, offset: 4 }, focus: { row: 5, offset: 1 } }
    expect(getRowSegmentBounds(sel, 2, 12)).toEqual({ selStart: 4, selEnd: 12 })
  })

  it('end line of multi-line: bounds are 0 .. end.offset', () => {
    const sel: Selection = { anchor: { row: 2, offset: 4 }, focus: { row: 5, offset: 8 } }
    expect(getRowSegmentBounds(sel, 5, 20)).toEqual({ selStart: 0, selEnd: 8 })
  })

  it('intermediate line: bounds are 0 .. lineLength', () => {
    const sel: Selection = { anchor: { row: 2, offset: 4 }, focus: { row: 5, offset: 8 } }
    expect(getRowSegmentBounds(sel, 3, 15)).toEqual({ selStart: 0, selEnd: 15 })
    expect(getRowSegmentBounds(sel, 4, 0)).toBeNull() // intermediate line with zero length
  })

  it('clamps offsets that exceed the line length', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 0, offset: 100 } }
    expect(getRowSegmentBounds(sel, 0, 5)).toEqual({ selStart: 0, selEnd: 5 })
  })

  it('returns null when bounds collapse on this line', () => {
    // Single-line selection with start == end on the line.
    const sel: Selection = { anchor: { row: 0, offset: 3 }, focus: { row: 0, offset: 3 } }
    expect(getRowSegmentBounds(sel, 0, 10)).toBeNull()
  })

  it('handles reversed selections', () => {
    const reversed: Selection = { anchor: { row: 4, offset: 6 }, focus: { row: 2, offset: 3 } }
    expect(getRowSegmentBounds(reversed, 2, 10)).toEqual({ selStart: 3, selEnd: 10 })
    expect(getRowSegmentBounds(reversed, 4, 10)).toEqual({ selStart: 0, selEnd: 6 })
  })

  it('preserves UTF-16 offsets across surrogate pairs (caller manages clamping)', () => {
    // The wave emoji "👋" is two UTF-16 units. We don't auto-clamp at this layer;
    // the segmenter trusts the offsets it gets from the caller (which clamps to
    // sane positions via caret math in M3a). Here we just verify the math is
    // unit-faithful: offset 1 inside "👋hello" gives selStart=1, selEnd=3.
    const sel: Selection = { anchor: { row: 0, offset: 1 }, focus: { row: 0, offset: 3 } }
    // "👋hello".length === 7 (2 for the emoji + 5 for "hello").
    expect(getRowSegmentBounds(sel, 0, 7)).toEqual({ selStart: 1, selEnd: 3 })
  })
})

describe('extendSelection (shift-click)', () => {
  it('no current selection: anchor = focus = point', () => {
    const point = { row: 5, offset: 2 }
    expect(extendSelection(null, point)).toEqual({ anchor: point, focus: point })
  })

  it('preserves existing anchor, moves focus to the new point', () => {
    const current: Selection = { anchor: { row: 2, offset: 3 }, focus: { row: 5, offset: 7 } }
    const newPoint = { row: 8, offset: 1 }
    expect(extendSelection(current, newPoint)).toEqual({
      anchor: { row: 2, offset: 3 },
      focus: { row: 8, offset: 1 },
    })
  })

  it('can shrink the selection (new focus before the anchor)', () => {
    const current: Selection = { anchor: { row: 5, offset: 0 }, focus: { row: 10, offset: 0 } }
    const newPoint = { row: 7, offset: 2 }
    expect(extendSelection(current, newPoint)).toEqual({
      anchor: { row: 5, offset: 0 },
      focus: { row: 7, offset: 2 },
    })
  })

  it('can flip the selection direction (new focus before the original anchor)', () => {
    const current: Selection = { anchor: { row: 5, offset: 0 }, focus: { row: 10, offset: 0 } }
    const newPoint = { row: 2, offset: 0 }
    expect(extendSelection(current, newPoint)).toEqual({
      anchor: { row: 5, offset: 0 },
      focus: { row: 2, offset: 0 },
    })
  })
})

describe('makeSelectAll', () => {
  it('returns null for 0-line files', () => {
    expect(makeSelectAll(0, 0)).toBeNull()
  })

  it('single-line file: anchor at (0,0), focus at (0, lastRowLength)', () => {
    expect(makeSelectAll(1, 42)).toEqual({
      anchor: { row: 0, offset: 0 },
      focus: { row: 0, offset: 42 },
    })
  })

  it('N-line file: focus at (N-1, lastRowLength)', () => {
    expect(makeSelectAll(10, 7)).toEqual({
      anchor: { row: 0, offset: 0 },
      focus: { row: 9, offset: 7 },
    })
  })

  it('"only newlines" file (three empty lines): focus at (2, 0)', () => {
    expect(makeSelectAll(3, 0)).toEqual({
      anchor: { row: 0, offset: 0 },
      focus: { row: 2, offset: 0 },
    })
  })
})

describe('makeSelectToEof', () => {
  it('runs from the file start to EOF_ROW', () => {
    expect(makeSelectToEof()).toEqual({
      anchor: { row: 0, offset: 0 },
      focus: { row: EOF_ROW, offset: 0 },
    })
  })

  it('sorts after every real line, so normalisation needs no special case', () => {
    const sel = makeSelectToEof()
    expect(normaliseSelection(sel)).toEqual({ start: sel.anchor, end: sel.focus })
  })
})

describe('toRangeEnds', () => {
  it('returns null for no selection', () => {
    expect(toRangeEnds(null)).toBeNull()
  })

  it('emits both ends as concrete lines for an ordinary selection', () => {
    const sel: Selection = { anchor: { row: 2, offset: 3 }, focus: { row: 7, offset: 1 } }
    expect(toRangeEnds(sel)).toEqual({
      anchor: { kind: 'row', row: 2, offset: 3 },
      focus: { kind: 'row', row: 7, offset: 1 },
    })
  })

  it('puts a reversed drag back in document order', () => {
    const sel: Selection = { anchor: { row: 7, offset: 1 }, focus: { row: 2, offset: 3 } }
    expect(toRangeEnds(sel)).toEqual({
      anchor: { kind: 'row', row: 2, offset: 3 },
      focus: { kind: 'row', row: 7, offset: 1 },
    })
  })

  it('maps the end-of-file selection to RangeEnd::Eof', () => {
    expect(toRangeEnds(makeSelectToEof())).toEqual({
      anchor: { kind: 'row', row: 0, offset: 0 },
      focus: { kind: 'eof' },
    })
  })
})

describe('selectionBytesFromFileSize', () => {
  /** A 100-row file of 4 096 bytes whose last row is "tail" (4 bytes). */
  const file = { totalRows: 100, totalBytes: 4_096, lastRowText: 'tail' }

  it('sizes a full ⌘A as the file itself', () => {
    expect(selectionBytesFromFileSize(makeSelectAll(100, 4), file)).toBe(4_096)
  })

  it('sizes an end-of-file selection as the file itself, count or no count', () => {
    const sel = makeSelectToEof()
    expect(selectionBytesFromFileSize(sel, { ...file, totalRows: null, lastRowText: null })).toBe(4_096)
    expect(selectionBytesFromFileSize(sel, file)).toBe(4_096)
  })

  it('subtracts what a selection stopping PARTWAY into the last row leaves behind', () => {
    // ❗ The I3 fix. This used to hand the copy band the whole file's size, so a
    // selection two characters short of the end was tiered as if it were the lot. The
    // number that picks confirm-versus-refuse is now the number that gets copied.
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 99, offset: 2 } }
    expect(selectionBytesFromFileSize(sel, file)).toBe(4_094)
  })

  it('counts leftover bytes, not leftover code units, when the tail is not ASCII', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 99, offset: 0 } }
    // "héllo" is 5 UTF-16 units and 6 UTF-8 bytes; none of it is selected.
    expect(selectionBytesFromFileSize(sel, { ...file, lastRowText: 'héllo' })).toBe(4_090)
  })

  it('declines when the last row is not cached, so the caller asks instead of guessing', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 99, offset: 0 } }
    expect(selectionBytesFromFileSize(sel, { ...file, lastRowText: null })).toBeNull()
  })

  it('declines a non-zero start row', () => {
    const sel: Selection = { anchor: { row: 1, offset: 0 }, focus: { row: 99, offset: 4 } }
    expect(selectionBytesFromFileSize(sel, file)).toBeNull()
  })

  it('declines a non-zero start offset', () => {
    const sel: Selection = { anchor: { row: 0, offset: 1 }, focus: { row: 99, offset: 4 } }
    expect(selectionBytesFromFileSize(sel, file)).toBeNull()
  })

  it('declines an end before the last row, which the per-row walk can size exactly', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 50, offset: 0 } }
    expect(selectionBytesFromFileSize(sel, file)).toBeNull()
  })

  it('normalises reversed selections', () => {
    const reversed: Selection = { anchor: { row: 99, offset: 4 }, focus: { row: 0, offset: 0 } }
    expect(selectionBytesFromFileSize(reversed, file)).toBe(4_096)
  })

  it('returns null for no selection', () => {
    expect(selectionBytesFromFileSize(null, file)).toBeNull()
  })

  it('without a row count and without an end-of-file focus, declines', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 99, offset: 5 } }
    expect(selectionBytesFromFileSize(sel, { ...file, totalRows: null })).toBeNull()
  })
})

describe('estimateSelectionBytes', () => {
  /**
   * A fixed per-line metrics lookup. `text` is the line's own UTF-8 bytes and `delimiter`
   * what follows it in the file, so a fixture says out loud which lines the file
   * delimits; `utf16` defaults to `text` (ASCII).
   */
  function makeLookup(lines: { text: number; utf16?: number; delimiter: number }[]) {
    return (n: number) =>
      n >= 0 && n < lines.length
        ? { textBytes: lines[n].text, utf16Length: lines[n].utf16 ?? lines[n].text, delimiterBytes: lines[n].delimiter }
        : null
  }

  it('returns 0 for empty or null selections', () => {
    const lookup = makeLookup([{ text: 9, delimiter: 1 }])
    expect(estimateSelectionBytes(null, lookup)).toBe(0)
    const collapsed: Selection = { anchor: { row: 0, offset: 3 }, focus: { row: 0, offset: 3 } }
    expect(estimateSelectionBytes(collapsed, lookup)).toBe(0)
  })

  it('single-line ASCII selection: counts the partial offset in bytes', () => {
    // "hello world\n": 11 text bytes and 11 UTF-16 units, plus its newline.
    const lookup = makeLookup([{ text: 11, delimiter: 1 }])
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 0, offset: 5 } }
    // 11 text bytes * (5 / 11) ≈ 5.
    expect(estimateSelectionBytes(sel, lookup)).toBe(5)
  })

  it('multi-line ASCII selection: sums bytes including newlines for full lines', () => {
    const lookup = makeLookup([
      { text: 5, delimiter: 1 }, // "hello\n"
      { text: 5, delimiter: 1 }, // "world\n"
      { text: 3, delimiter: 1 }, // "foo\n"
    ])
    // From (0, 2) to (2, 3): "llo\n" + "world\n" + "foo".
    const sel: Selection = { anchor: { row: 0, offset: 2 }, focus: { row: 2, offset: 3 } }
    // line 0 partial: 5 text bytes * (3/5) = 3, + 1 newline = 4.
    // line 1 full: 6.
    // line 2 partial: 3 text bytes * (3/3) = 3.
    // total = 13.
    expect(estimateSelectionBytes(sel, lookup)).toBe(13)
  })

  it('end at offset 0 contributes nothing from the end line', () => {
    const lookup = makeLookup([
      { text: 3, delimiter: 1 }, // "abc\n"
      { text: 3, delimiter: 1 }, // "def\n"
    ])
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 1, offset: 0 } }
    // line 0 full text: 3 bytes + 1 newline = 4. line 1 contributes 0.
    expect(estimateSelectionBytes(sel, lookup)).toBe(4)
  })

  it('"only newlines" file: full select returns sum of newlines minus the last', () => {
    const lookup = makeLookup([
      { text: 0, delimiter: 1 }, // "\n"
      { text: 0, delimiter: 1 }, // "\n"
      { text: 0, delimiter: 1 }, // "\n"
    ])
    // Select all: (0,0) to (2, 0). Lines 0,1 contribute 1 byte (newline) each, line 2 contributes 0.
    const sel = makeSelectAll(3, 0)
    expect(sel).not.toBeNull()
    expect(estimateSelectionBytes(sel, lookup)).toBe(2)
  })

  it('multi-byte UTF-8 line: scales bytes by UTF-16 ratio', () => {
    // A line with one wave emoji "👋" then "hi": UTF-8 = 4 + 2 = 6 bytes,
    // UTF-16 = 2 (emoji surrogate pair) + 2 = 4.
    const lookup = makeLookup([{ text: 6, utf16: 4, delimiter: 1 }])
    // Select just the emoji (offsets 0..2): 6 text bytes * (2/4) = 3 (rounded).
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 0, offset: 2 } }
    expect(estimateSelectionBytes(sel, lookup)).toBe(3)
  })

  it('returns null when any required line length is unknown', () => {
    const lookup = makeLookup([{ text: 9, delimiter: 1 }])
    // line 2 isn't in the lookup; selection ends there → null.
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 1 } }
    expect(estimateSelectionBytes(sel, lookup)).toBeNull()
  })

  it('does not assume a delimiter the file does not have', () => {
    // "abc": one line, three bytes, nothing after them. The whole line is three bytes,
    // not two: there is no newline to leave out.
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 0, offset: 3 } }
    expect(estimateSelectionBytes(sel, makeLookup([{ text: 3, delimiter: 0 }]))).toBe(3)
  })

  it('counts no delimiter at all when every row is one the viewer broke itself', () => {
    // Where this is heading: a long line becomes several rows, and a row that ends at a
    // segment boundary has NO delimiter after it. Three 5-byte rows selected whole are
    // 15 bytes, not 15 plus a newline per row.
    const lookup = makeLookup([
      { text: 5, delimiter: 0 },
      { text: 5, delimiter: 0 },
      { text: 5, delimiter: 0 },
    ])
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 5 } }
    expect(estimateSelectionBytes(sel, lookup)).toBe(15)
  })

  it('counts a CRLF delimiter as the one byte the backend leaves outside the line text', () => {
    // All three backends keep the `\r` AS PART of the line text and split on `\n` alone,
    // so "ab\r\ncd\r\n" is two 3-byte lines with a 1-byte delimiter each, not a 2-byte one.
    const lookup = makeLookup([
      { text: 3, delimiter: 1 }, // "ab\r"
      { text: 3, delimiter: 1 }, // "cd\r"
    ])
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 1, offset: 3 } }
    expect(estimateSelectionBytes(sel, lookup)).toBe(7)
  })

  it('reversed selection: same result as the normalised version', () => {
    const lookup = makeLookup([
      { text: 5, delimiter: 1 },
      { text: 5, delimiter: 1 },
    ])
    const forward: Selection = { anchor: { row: 0, offset: 1 }, focus: { row: 1, offset: 4 } }
    const reversed: Selection = { anchor: { row: 1, offset: 4 }, focus: { row: 0, offset: 1 } }
    expect(estimateSelectionBytes(forward, lookup)).toBe(estimateSelectionBytes(reversed, lookup))
  })
})

describe('describeSelectionForAt', () => {
  // Test-local row lookups. Rows are one-to-one with lines here (nothing wraps), which
  // is the ordinary-file case; the wrapped case has its own block below.
  const allOnes = (row: number): AnnouncedRow | null => ({ utf16Length: 1, lineNumber: row })
  const empty = (): AnnouncedRow | null => null

  it('null selection returns empty string', () => {
    expect(describeSelectionForAt(null, empty)).toBe('')
  })

  it('caret-only (start == end) returns empty string', () => {
    const sel: Selection = { anchor: { row: 3, offset: 4 }, focus: { row: 3, offset: 4 } }
    expect(describeSelectionForAt(sel, empty)).toBe('')
  })

  it('single-line selection: announces character count and line number (1-indexed)', () => {
    const sel: Selection = { anchor: { row: 4, offset: 2 }, focus: { row: 4, offset: 7 } }
    expect(describeSelectionForAt(sel, allOnes)).toBe('Selected 5 characters on line 5')
  })

  it('multi-line selection: announces line range and total chars', () => {
    // Lines 0..3, each "hello" (5 chars). Select from (0,2) to (3,3):
    //   line 0 contributes "llo" (3), line 1 + 2 each contribute 5, line 3 contributes 3. Total 16.
    const sel: Selection = { anchor: { row: 0, offset: 2 }, focus: { row: 3, offset: 3 } }
    const getRow = (n: number): AnnouncedRow | null => (n >= 0 && n < 4 ? { utf16Length: 5, lineNumber: n } : null)
    expect(describeSelectionForAt(sel, getRow)).toBe('Selected lines 1 to 4, 16 characters')
  })

  it('end-of-file selection: line span > MAX_ANNOUNCE_ROWS falls back to generic message', () => {
    // ⌘A with no line count yet reaches EOF_ROW, so the pure function must not
    // iterate 9e15 times.
    const sel = makeSelectToEof()
    let calls = 0
    const counting = (row: number): AnnouncedRow | null => {
      calls++
      return { utf16Length: 1, lineNumber: row }
    }
    expect(describeSelectionForAt(sel, counting)).toBe('Selected from line 1 to the end of the file')
    // The fallback path must not walk the rows at all: row 0 starts line 0 by the row
    // rule, so even the line lookup answers without touching the cache.
    expect(calls).toBe(0)
  })

  it('end-of-file selection from an unresolvable line drops the line rather than guessing', () => {
    // A huge drag starting mid-way through a long line whose line-start row has been
    // evicted. Naming a row number here would be meaningless to a listener.
    const sel: Selection = { anchor: { row: 500, offset: 0 }, focus: { row: EOF_ROW, offset: 0 } }
    expect(describeSelectionForAt(sel, empty)).toBe('Selected to the end of the file')
  })

  it('line span exactly at the cap (MAX_ANNOUNCE_ROWS) still itemises', () => {
    const sel: Selection = {
      anchor: { row: 0, offset: 0 },
      focus: { row: MAX_ANNOUNCE_ROWS, offset: 0 },
    }
    // Just verify we don't fall back; the exact char count isn't the point.
    const result = describeSelectionForAt(sel, allOnes)
    expect(result.startsWith('Selected lines')).toBe(true)
  })

  it('reversed selection: same result as the normalised version', () => {
    const forward: Selection = { anchor: { row: 1, offset: 0 }, focus: { row: 3, offset: 2 } }
    const reversed: Selection = { anchor: { row: 3, offset: 2 }, focus: { row: 1, offset: 0 } }
    expect(describeSelectionForAt(forward, allOnes)).toBe(describeSelectionForAt(reversed, allOnes))
  })

  it('missing row length in lookup contributes 0 to the count (degrades gracefully)', () => {
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 0 } }
    // Lines are known, lengths are not: row 0 contributes (0 - 0) = 0, row 1 contributes
    // 0, row 2 contributes 0.
    const lengthless = (row: number): AnnouncedRow | null => ({ utf16Length: 0, lineNumber: row })
    expect(describeSelectionForAt(sel, lengthless)).toBe('Selected lines 1 to 3, 0 characters')
  })

  it('drops the location when the physical line cannot be resolved', () => {
    // Nothing cached above the selection, so the row that starts the line is unknown. A
    // row number here would name a coordinate the gutter never shows.
    const sel: Selection = { anchor: { row: 40, offset: 0 }, focus: { row: 41, offset: 3 } }
    expect(describeSelectionForAt(sel, empty)).toBe('Selected 3 characters')
  })

  it('uses the singular for one character', () => {
    const sel: Selection = { anchor: { row: 40, offset: 0 }, focus: { row: 40, offset: 1 } }
    expect(describeSelectionForAt(sel, empty)).toBe('Selected 1 character')
    expect(describeSelectionForAt(sel, allOnes)).toBe('Selected 1 character on line 41')
  })

  it('groups large character counts the way the locale does', () => {
    const wide = (row: number): AnnouncedRow | null => ({ utf16Length: 20_000, lineNumber: row })
    const sel: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 1, offset: 12_345 } }
    expect(describeSelectionForAt(sel, wide)).toBe('Selected lines 1 to 2, 32,345 characters')
  })
})

describe('describeSelectionForAt on a wrapped line', () => {
  /**
   * One 60 000-character physical line (line 0) occupying rows 0, 1, and 2, then an
   * ordinary second line (line 1) on row 3. Exactly what the gutter draws: "1" beside
   * row 0, nothing beside rows 1 and 2, "2" beside row 3.
   */
  const rows: Record<number, AnnouncedRow> = {
    0: { utf16Length: 20_000, lineNumber: 0 },
    1: { utf16Length: 20_000, lineNumber: null },
    2: { utf16Length: 20_000, lineNumber: null },
    3: { utf16Length: 5, lineNumber: 1 },
  }
  const wrapped = (row: number): AnnouncedRow | null => rows[row] ?? null

  it('names the physical line, not the row, for a selection inside one wrapped line', () => {
    // Rows 1 and 2 are continuations of line 0, so the user sees a blank gutter beside
    // them and "1" above. Announcing "lines 2 to 3" names a coordinate the file does not
    // have and the screen does not show.
    const sel: Selection = { anchor: { row: 1, offset: 0 }, focus: { row: 2, offset: 5 } }
    expect(describeSelectionForAt(sel, wrapped)).toBe('Selected 20,005 characters on line 1')
  })

  it('names the physical line range when the selection really does cross lines', () => {
    const sel: Selection = { anchor: { row: 2, offset: 19_998 }, focus: { row: 3, offset: 3 } }
    expect(describeSelectionForAt(sel, wrapped)).toBe('Selected lines 1 to 2, 5 characters')
  })
})
