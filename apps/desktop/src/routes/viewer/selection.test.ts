import { describe, it, expect } from 'vitest'

import {
  compareLineOffset,
  estimateSelectionBytes,
  extendSelection,
  getLineSegmentBounds,
  isEmpty,
  isLineInRange,
  lineOffsetEquals,
  makeSelectAll,
  normaliseSelection,
  type Selection,
} from './selection.svelte'

describe('compareLineOffset', () => {
  it('returns 0 for identical points', () => {
    expect(compareLineOffset({ line: 3, offset: 5 }, { line: 3, offset: 5 })).toBe(0)
  })

  it('compares by line first', () => {
    expect(compareLineOffset({ line: 2, offset: 99 }, { line: 3, offset: 0 })).toBeLessThan(0)
    expect(compareLineOffset({ line: 5, offset: 0 }, { line: 2, offset: 99 })).toBeGreaterThan(0)
  })

  it('compares by offset when lines match', () => {
    expect(compareLineOffset({ line: 4, offset: 1 }, { line: 4, offset: 7 })).toBeLessThan(0)
    expect(compareLineOffset({ line: 4, offset: 7 }, { line: 4, offset: 1 })).toBeGreaterThan(0)
  })
})

describe('lineOffsetEquals', () => {
  it('returns true for identical points', () => {
    expect(lineOffsetEquals({ line: 0, offset: 0 }, { line: 0, offset: 0 })).toBe(true)
  })
  it('returns false when lines differ', () => {
    expect(lineOffsetEquals({ line: 0, offset: 5 }, { line: 1, offset: 5 })).toBe(false)
  })
  it('returns false when offsets differ', () => {
    expect(lineOffsetEquals({ line: 7, offset: 1 }, { line: 7, offset: 2 })).toBe(false)
  })
})

describe('normaliseSelection', () => {
  it('returns endpoints unchanged when already in order', () => {
    const sel: Selection = { anchor: { line: 0, offset: 0 }, focus: { line: 3, offset: 4 } }
    const { start, end } = normaliseSelection(sel)
    expect(start).toEqual({ line: 0, offset: 0 })
    expect(end).toEqual({ line: 3, offset: 4 })
  })

  it('swaps endpoints when reversed (anchor below focus)', () => {
    const sel: Selection = { anchor: { line: 5, offset: 2 }, focus: { line: 1, offset: 8 } }
    const { start, end } = normaliseSelection(sel)
    expect(start).toEqual({ line: 1, offset: 8 })
    expect(end).toEqual({ line: 5, offset: 2 })
  })

  it('handles same-line reversed selection', () => {
    const sel: Selection = { anchor: { line: 2, offset: 10 }, focus: { line: 2, offset: 3 } }
    const { start, end } = normaliseSelection(sel)
    expect(start).toEqual({ line: 2, offset: 3 })
    expect(end).toEqual({ line: 2, offset: 10 })
  })
})

describe('isEmpty', () => {
  it('null is empty', () => {
    expect(isEmpty(null)).toBe(true)
  })

  it('anchor == focus is empty (caret-only click)', () => {
    expect(isEmpty({ anchor: { line: 2, offset: 5 }, focus: { line: 2, offset: 5 } })).toBe(true)
  })

  it('different endpoints means not empty', () => {
    expect(isEmpty({ anchor: { line: 0, offset: 0 }, focus: { line: 0, offset: 1 } })).toBe(false)
  })
})

describe('isLineInRange', () => {
  const sel: Selection = { anchor: { line: 2, offset: 3 }, focus: { line: 5, offset: 7 } }

  it('returns false for lines before the start', () => {
    expect(isLineInRange(sel, 0)).toBe(false)
    expect(isLineInRange(sel, 1)).toBe(false)
  })

  it('returns true for start line, end line, and intermediate lines', () => {
    expect(isLineInRange(sel, 2)).toBe(true)
    expect(isLineInRange(sel, 3)).toBe(true)
    expect(isLineInRange(sel, 4)).toBe(true)
    expect(isLineInRange(sel, 5)).toBe(true)
  })

  it('returns false for lines after the end', () => {
    expect(isLineInRange(sel, 6)).toBe(false)
    expect(isLineInRange(sel, 100)).toBe(false)
  })

  it('returns false for empty selections', () => {
    expect(isLineInRange(null, 5)).toBe(false)
    expect(isLineInRange({ anchor: { line: 5, offset: 0 }, focus: { line: 5, offset: 0 } }, 5)).toBe(false)
  })

  it('works with reversed selections (anchor below focus)', () => {
    const reversed: Selection = { anchor: { line: 5, offset: 0 }, focus: { line: 2, offset: 0 } }
    expect(isLineInRange(reversed, 3)).toBe(true)
    expect(isLineInRange(reversed, 1)).toBe(false)
  })
})

describe('getLineSegmentBounds', () => {
  it('returns null for empty selections', () => {
    expect(getLineSegmentBounds(null, 0, 10)).toBeNull()
  })

  it('returns null for lines outside the range', () => {
    const sel: Selection = { anchor: { line: 2, offset: 0 }, focus: { line: 4, offset: 5 } }
    expect(getLineSegmentBounds(sel, 1, 10)).toBeNull()
    expect(getLineSegmentBounds(sel, 5, 10)).toBeNull()
  })

  it('single-line selection: bounds are start.offset .. end.offset', () => {
    const sel: Selection = { anchor: { line: 3, offset: 2 }, focus: { line: 3, offset: 7 } }
    expect(getLineSegmentBounds(sel, 3, 20)).toEqual({ selStart: 2, selEnd: 7 })
  })

  it('start line of multi-line: bounds are start.offset .. lineLength', () => {
    const sel: Selection = { anchor: { line: 2, offset: 4 }, focus: { line: 5, offset: 1 } }
    expect(getLineSegmentBounds(sel, 2, 12)).toEqual({ selStart: 4, selEnd: 12 })
  })

  it('end line of multi-line: bounds are 0 .. end.offset', () => {
    const sel: Selection = { anchor: { line: 2, offset: 4 }, focus: { line: 5, offset: 8 } }
    expect(getLineSegmentBounds(sel, 5, 20)).toEqual({ selStart: 0, selEnd: 8 })
  })

  it('intermediate line: bounds are 0 .. lineLength', () => {
    const sel: Selection = { anchor: { line: 2, offset: 4 }, focus: { line: 5, offset: 8 } }
    expect(getLineSegmentBounds(sel, 3, 15)).toEqual({ selStart: 0, selEnd: 15 })
    expect(getLineSegmentBounds(sel, 4, 0)).toBeNull() // intermediate line with zero length
  })

  it('clamps offsets that exceed the line length', () => {
    const sel: Selection = { anchor: { line: 0, offset: 0 }, focus: { line: 0, offset: 100 } }
    expect(getLineSegmentBounds(sel, 0, 5)).toEqual({ selStart: 0, selEnd: 5 })
  })

  it('returns null when bounds collapse on this line', () => {
    // Single-line selection with start == end on the line.
    const sel: Selection = { anchor: { line: 0, offset: 3 }, focus: { line: 0, offset: 3 } }
    expect(getLineSegmentBounds(sel, 0, 10)).toBeNull()
  })

  it('handles reversed selections', () => {
    const reversed: Selection = { anchor: { line: 4, offset: 6 }, focus: { line: 2, offset: 3 } }
    expect(getLineSegmentBounds(reversed, 2, 10)).toEqual({ selStart: 3, selEnd: 10 })
    expect(getLineSegmentBounds(reversed, 4, 10)).toEqual({ selStart: 0, selEnd: 6 })
  })

  it('preserves UTF-16 offsets across surrogate pairs (caller manages clamping)', () => {
    // The wave emoji "👋" is two UTF-16 units. We don't auto-clamp at this layer;
    // the segmenter trusts the offsets it gets from the caller (which clamps to
    // sane positions via caret math in M3a). Here we just verify the math is
    // unit-faithful: offset 1 inside "👋hello" gives selStart=1, selEnd=3.
    const sel: Selection = { anchor: { line: 0, offset: 1 }, focus: { line: 0, offset: 3 } }
    // "👋hello".length === 7 (2 for the emoji + 5 for "hello").
    expect(getLineSegmentBounds(sel, 0, 7)).toEqual({ selStart: 1, selEnd: 3 })
  })
})

describe('extendSelection (shift-click)', () => {
  it('no current selection: anchor = focus = point', () => {
    const point = { line: 5, offset: 2 }
    expect(extendSelection(null, point)).toEqual({ anchor: point, focus: point })
  })

  it('preserves existing anchor, moves focus to the new point', () => {
    const current: Selection = { anchor: { line: 2, offset: 3 }, focus: { line: 5, offset: 7 } }
    const newPoint = { line: 8, offset: 1 }
    expect(extendSelection(current, newPoint)).toEqual({
      anchor: { line: 2, offset: 3 },
      focus: { line: 8, offset: 1 },
    })
  })

  it('can shrink the selection (new focus before the anchor)', () => {
    const current: Selection = { anchor: { line: 5, offset: 0 }, focus: { line: 10, offset: 0 } }
    const newPoint = { line: 7, offset: 2 }
    expect(extendSelection(current, newPoint)).toEqual({
      anchor: { line: 5, offset: 0 },
      focus: { line: 7, offset: 2 },
    })
  })

  it('can flip the selection direction (new focus before the original anchor)', () => {
    const current: Selection = { anchor: { line: 5, offset: 0 }, focus: { line: 10, offset: 0 } }
    const newPoint = { line: 2, offset: 0 }
    expect(extendSelection(current, newPoint)).toEqual({
      anchor: { line: 5, offset: 0 },
      focus: { line: 2, offset: 0 },
    })
  })
})

describe('makeSelectAll', () => {
  it('returns null for 0-line files', () => {
    expect(makeSelectAll(0, 0)).toBeNull()
  })

  it('single-line file: anchor at (0,0), focus at (0, lastLineLength)', () => {
    expect(makeSelectAll(1, 42)).toEqual({
      anchor: { line: 0, offset: 0 },
      focus: { line: 0, offset: 42 },
    })
  })

  it('N-line file: focus at (N-1, lastLineLength)', () => {
    expect(makeSelectAll(10, 7)).toEqual({
      anchor: { line: 0, offset: 0 },
      focus: { line: 9, offset: 7 },
    })
  })

  it('"only newlines" file (three empty lines): focus at (2, 0)', () => {
    expect(makeSelectAll(3, 0)).toEqual({
      anchor: { line: 0, offset: 0 },
      focus: { line: 2, offset: 0 },
    })
  })
})

describe('estimateSelectionBytes', () => {
  // Helper that builds fixed byte / UTF-16 lookups.
  function makeLookups(lines: { bytes: number; utf16: number }[]) {
    return {
      getBytes: (n: number) => (n >= 0 && n < lines.length ? lines[n].bytes : null),
      getUtf16: (n: number) => (n >= 0 && n < lines.length ? lines[n].utf16 : null),
    }
  }

  it('returns 0 for empty or null selections', () => {
    const { getBytes, getUtf16 } = makeLookups([{ bytes: 10, utf16: 9 }])
    expect(estimateSelectionBytes(null, getBytes, getUtf16)).toBe(0)
    const collapsed: Selection = { anchor: { line: 0, offset: 3 }, focus: { line: 0, offset: 3 } }
    expect(estimateSelectionBytes(collapsed, getBytes, getUtf16)).toBe(0)
  })

  it('single-line ASCII selection: counts the partial offset in bytes', () => {
    // "hello world\n" = 12 bytes, 11 UTF-16 units (excl. newline).
    const { getBytes, getUtf16 } = makeLookups([{ bytes: 12, utf16: 11 }])
    const sel: Selection = { anchor: { line: 0, offset: 0 }, focus: { line: 0, offset: 5 } }
    // 11 text bytes * (5 / 11) ≈ 5.
    expect(estimateSelectionBytes(sel, getBytes, getUtf16)).toBe(5)
  })

  it('multi-line ASCII selection: sums bytes including newlines for full lines', () => {
    const lines = [
      { bytes: 6, utf16: 5 }, // "hello\n"
      { bytes: 6, utf16: 5 }, // "world\n"
      { bytes: 4, utf16: 3 }, // "foo\n"
    ]
    const { getBytes, getUtf16 } = makeLookups(lines)
    // From (0, 2) to (2, 3): "llo\n" + "world\n" + "foo".
    const sel: Selection = { anchor: { line: 0, offset: 2 }, focus: { line: 2, offset: 3 } }
    // line 0 partial: 5 text bytes * (3/5) = 3, + 1 newline = 4.
    // line 1 full: 6.
    // line 2 partial: 3 text bytes * (3/3) = 3.
    // total = 13.
    expect(estimateSelectionBytes(sel, getBytes, getUtf16)).toBe(13)
  })

  it('end at offset 0 contributes nothing from the end line', () => {
    const lines = [
      { bytes: 4, utf16: 3 }, // "abc\n"
      { bytes: 4, utf16: 3 }, // "def\n"
    ]
    const { getBytes, getUtf16 } = makeLookups(lines)
    const sel: Selection = { anchor: { line: 0, offset: 0 }, focus: { line: 1, offset: 0 } }
    // line 0 full text: 3 bytes + 1 newline = 4. line 1 contributes 0.
    expect(estimateSelectionBytes(sel, getBytes, getUtf16)).toBe(4)
  })

  it('"only newlines" file: full select returns sum of newlines minus the last', () => {
    const lines = [
      { bytes: 1, utf16: 0 }, // "\n"
      { bytes: 1, utf16: 0 }, // "\n"
      { bytes: 1, utf16: 0 }, // "\n"
    ]
    const { getBytes, getUtf16 } = makeLookups(lines)
    // Select all: (0,0) to (2, 0). Lines 0,1 contribute 1 byte (newline) each, line 2 contributes 0.
    const sel = makeSelectAll(3, 0)
    expect(sel).not.toBeNull()
    expect(estimateSelectionBytes(sel, getBytes, getUtf16)).toBe(2)
  })

  it('multi-byte UTF-8 line: scales bytes by UTF-16 ratio', () => {
    // A line with one wave emoji "👋" then "hi": UTF-8 = 4 + 2 = 6 bytes (+1 newline) = 7,
    // UTF-16 = 2 (emoji surrogate pair) + 2 = 4.
    const { getBytes, getUtf16 } = makeLookups([{ bytes: 7, utf16: 4 }])
    // Select just the emoji (offsets 0..2): 6 text bytes * (2/4) = 3 (rounded).
    const sel: Selection = { anchor: { line: 0, offset: 0 }, focus: { line: 0, offset: 2 } }
    expect(estimateSelectionBytes(sel, getBytes, getUtf16)).toBe(3)
  })

  it('returns null when any required line length is unknown', () => {
    const { getBytes, getUtf16 } = makeLookups([{ bytes: 10, utf16: 9 }])
    // line 2 isn't in the lookup; selection ends there → null.
    const sel: Selection = { anchor: { line: 0, offset: 0 }, focus: { line: 2, offset: 1 } }
    expect(estimateSelectionBytes(sel, getBytes, getUtf16)).toBeNull()
  })

  it('reversed selection: same result as the normalised version', () => {
    const lines = [
      { bytes: 6, utf16: 5 },
      { bytes: 6, utf16: 5 },
    ]
    const { getBytes, getUtf16 } = makeLookups(lines)
    const forward: Selection = { anchor: { line: 0, offset: 1 }, focus: { line: 1, offset: 4 } }
    const reversed: Selection = { anchor: { line: 1, offset: 4 }, focus: { line: 0, offset: 1 } }
    expect(estimateSelectionBytes(forward, getBytes, getUtf16)).toBe(
      estimateSelectionBytes(reversed, getBytes, getUtf16),
    )
  })
})
