/**
 * Characterization tests: what select-all and the copy-size arithmetic do TODAY.
 *
 * The viewer is about to stop counting physical lines and start counting bounded rows
 * (`src-tauri/src/file_viewer/DETAILS.md` § "The row invariants"). Invariant I6 says a file whose every line is
 * shorter than the segment size must behave byte-identically afterwards, and these
 * tests are the register that claim is checked against.
 *
 * ❗ These pin REALITY, including behaviour that is wrong. Where today's answer is a
 * bug, the test says so in its name and its comment and asserts the buggy value anyway.
 * A rewrite that changes one of these has to change it deliberately, which is the whole
 * point. ❌ Don't "fix" a test here by deleting it.
 */

import { describe, expect, it, vi } from 'vitest'

import {
  describeSelectionForAt,
  estimateSelectionBytes,
  rowMetrics,
  selectionBytesFromFileSize,
  makeSelectAll,
  makeSelectToEof,
  toRangeEnds,
  EOF_ROW,
  type Selection,
} from './selection.svelte'
import { createViewerKeyboard } from './viewer-keyboard'

type KeyboardDeps = Parameters<typeof createViewerKeyboard>[0]

/**
 * A keyboard dep set wired for ⌘A only: the line count, the per-line text, and a
 * `selectAll` / `selectToEof` pair of spies. Everything else is a no-op, so a test
 * reads as the one question it asks.
 */
function selectAllDeps(overrides: Partial<KeyboardDeps> = {}) {
  const noop = vi.fn()
  const selectAll = vi.fn()
  const selectToEof = vi.fn()
  const deps = {
    getTotalRows: () => 3,
    getTotalBytes: () => 16,
    getRowText: () => 'line',
    getLastRenderedRow: () => 2,
    selection: { selection: null, selectAll, selectToEof, setFocus: noop },
    scroll: {
      scrollByRows: noop,
      scrollByPages: noop,
      scrollToStart: noop,
      scrollToEnd: noop,
      scrollByColumns: noop,
      ensureRowVisible: noop,
      ensureColumnVisible: noop,
    },
    search: {
      searchVisible: false,
      searchStatus: 'idle' as const,
      searchInputRef: null,
      openSearch: noop,
      closeSearch: noop,
      stopSearch: noop,
      findNext: noop,
      findPrev: noop,
      toggleUseRegex: noop,
      toggleCaseSensitive: noop,
    },
    copy: { busy: false, cancelInFlight: () => Promise.resolve() },
    ...overrides,
  } as unknown as KeyboardDeps
  return { keyboard: createViewerKeyboard(deps), selectAll, selectToEof }
}

/**
 * The per-ROW metrics lookup `+page.svelte` hands `estimateSelectionBytes`, built from
 * the SHIPPING `rowMetrics` rather than a copy of it, so a change to how a delimiter is
 * decided fails here instead of drifting past.
 *
 * `continued` names the rows Cmdr broke out of a long line, the way the backend's
 * `continues` flag does; every other row is followed by the newline the file holds.
 */
function rowLookup(rows: string[], continued: number[] = []) {
  return (n: number) =>
    n >= 0 && n < rows.length
      ? rowMetrics({ text: rows[n], continues: continued.includes(n), isLastRow: n >= rows.length - 1 })
      : null
}

describe('select-all, as it behaves today', () => {
  it('ends the selection at the last line, at that line length', () => {
    // "alpha\nbeta\ngamma" with no trailing newline: three lines, the last one 5 long.
    expect(makeSelectAll(3, 5)).toEqual({ anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 5 } })
  })

  it('ends at offset 0 of a trailing empty line when the file DOES end in a newline', () => {
    // Every backend reports a trailing empty line for a file ending in `\n`, so ⌘A on
    // "alpha\nbeta\ngamma\n" is 4 lines with a 0-length last one. The range that reaches
    // the backend therefore stops at the newline, not past it.
    expect(makeSelectAll(4, 0)).toEqual({ anchor: { row: 0, offset: 0 }, focus: { row: 3, offset: 0 } })
  })

  it('is a no-op on an empty file', () => {
    expect(makeSelectAll(0, 0)).toBeNull()
  })

  it('reaches the backend as two line endpoints, or as `eof` when the count is unknown', () => {
    expect(toRangeEnds(makeSelectAll(3, 5))).toEqual({
      anchor: { kind: 'row', row: 0, offset: 0 },
      focus: { kind: 'row', row: 2, offset: 5 },
    })
    expect(toRangeEnds(makeSelectToEof())).toEqual({
      anchor: { kind: 'row', row: 0, offset: 0 },
      focus: { kind: 'eof' },
    })
  })

  it('takes the eof path when the line count is not known yet', () => {
    const { keyboard, selectAll, selectToEof } = selectAllDeps({ getTotalRows: () => null })
    keyboard.handleSelectAllShortcut()
    expect(selectAll).not.toHaveBeenCalled()
    expect(selectToEof).toHaveBeenCalledOnce()
  })

  it('does nothing at all when there is neither a line count nor a byte count', () => {
    const { keyboard, selectAll, selectToEof } = selectAllDeps({ getTotalRows: () => null, getTotalBytes: () => 0 })
    keyboard.handleSelectAllShortcut()
    expect(selectAll).not.toHaveBeenCalled()
    expect(selectToEof).not.toHaveBeenCalled()
  })

  it('passes the last line length through when the last line IS cached', () => {
    const { keyboard, selectAll } = selectAllDeps({ getRowText: (n: number) => ['alpha', 'beta', 'gamma'][n] })
    keyboard.handleSelectAllShortcut()
    expect(selectAll).toHaveBeenCalledWith({ totalRows: 3, lastRowLength: 5 })
  })

  it('takes the eof path for an uncached last line, rather than inventing a zero length', () => {
    // ❗ This pin CHANGED with the fix it was pinning. It used to assert
    // `{ totalRows: 3, lastRowLength: 0 }`: `viewer-keyboard.ts` read an uncached last
    // line as empty, so ⌘A on a long file the user hadn't scrolled to the end of ended
    // at offset 0 of that line and ⌘C copied the file minus its last line, quietly.
    // `RangeEnd::Eof` is the path that already existed for "we don't know where the end
    // is", and an absent last line is exactly that.
    const { keyboard, selectAll, selectToEof } = selectAllDeps({
      getRowText: (n: number) => (n === 2 ? undefined : 'alpha'),
    })
    keyboard.handleSelectAllShortcut()
    expect(selectAll).not.toHaveBeenCalled()
    expect(selectToEof).toHaveBeenCalledOnce()
  })

  it('sizes a whole-file selection from the file itself, in either of its two shapes', () => {
    // ❗ This pin CHANGED, deliberately. It used to assert a BOOLEAN
    // (`isWholeFileSelection`), and the copy flow turned a `true` into `totalBytes` — for
    // any selection reaching the last row, including one that stopped partway into it.
    // That is invariant I3 unmet: the same number picks the confirm dialog and the
    // refusal. The predicate is now the measurement, and it subtracts the leftover.
    const file = { totalRows: 3, totalBytes: 16, lastRowText: 'gamma' }
    expect(selectionBytesFromFileSize(makeSelectAll(3, 5), file)).toBe(16)
    expect(selectionBytesFromFileSize(makeSelectToEof(), { ...file, totalRows: null, lastRowText: null })).toBe(16)
    // Two characters short of the end is two bytes short of the file, not the whole file.
    expect(selectionBytesFromFileSize({ anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 3 } }, file)).toBe(14)
    // A selection that starts anywhere but the very top declines the shortcut, so the
    // copy flow walks rows for its size instead.
    expect(
      selectionBytesFromFileSize({ anchor: { row: 0, offset: 1 }, focus: { row: EOF_ROW, offset: 0 } }, file),
    ).toBeNull()
  })
})

describe('copy size arithmetic, as it behaves today', () => {
  // "alpha\nbeta\ngamma", 16 bytes, no trailing newline.
  const noTrailingNewline = ['alpha', 'beta', 'gamma']

  function estimate(sel: Selection, rows: string[], continued: number[] = []): number | null {
    return estimateSelectionBytes(sel, rowLookup(rows, continued))
  }

  it('sizes a whole-file selection of a file with NO trailing newline exactly', () => {
    // Three lines and the two newlines between them: 16 bytes, no newline invented after
    // the last line. Pinned because the row rewrite makes a continuation row's delimiter
    // 0 as well, and this is the number that must NOT move when it does.
    expect(estimate({ anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 5 } }, noTrailingNewline)).toBe(16)
  })

  it('sizes a whole-file selection of a file WITH a trailing newline exactly', () => {
    // Same three lines plus the trailing empty line every backend reports: 17 bytes.
    expect(estimate({ anchor: { row: 0, offset: 0 }, focus: { row: 3, offset: 0 } }, [...noTrailingNewline, ''])).toBe(
      17,
    )
  })

  it('sizes a selection inside one line', () => {
    expect(estimate({ anchor: { row: 0, offset: 1 }, focus: { row: 0, offset: 4 } }, noTrailingNewline)).toBe(3)
  })

  it('sizes a selection spanning lines, newline bytes included', () => {
    // "pha\nbeta\ngam" = 3 + 1 + 4 + 1 + 3.
    expect(estimate({ anchor: { row: 0, offset: 2 }, focus: { row: 2, offset: 3 } }, noTrailingNewline)).toBe(12)
  })

  it('is an approximation on a partial selection of a non-ASCII line, by design', () => {
    // "héllo" is 5 UTF-16 units and 6 UTF-8 bytes; the estimator prorates bytes by the
    // UTF-16 fraction rather than measuring, so "hé" (3 bytes) comes out as 2. The
    // comment in `selection.svelte.ts` calls this out: the number feeds the 10 MB
    // confirm and 100 MB refuse tiers, which need order-of-magnitude, not exactness.
    expect(estimate({ anchor: { row: 0, offset: 0 }, focus: { row: 0, offset: 2 } }, ['héllo'])).toBe(2)
  })

  it('returns null when a line the walk needs is not cached', () => {
    expect(
      estimateSelectionBytes({ anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 5 } }, () => null),
    ).toBeNull()
  })

  it('sizes an empty selection as zero', () => {
    expect(estimate({ anchor: { row: 1, offset: 2 }, focus: { row: 1, offset: 2 } }, noTrailingNewline)).toBe(0)
  })
})

/**
 * A Cmdr break is NOT a newline. Nothing that reconstructs or measures text from cached
 * rows may put a byte or a character between two rows of the same physical line.
 *
 * The file here is one 15-byte line with no newline in it at all, drawn as three rows of
 * five: exactly what a minified bundle looks like once `SEGMENT_BYTES` bites. Rows 0 and
 * 1 are continuations; row 2 ends the line (and the file).
 */
describe('a file whose rows Cmdr made, not the file', () => {
  const minified = ['aaaaa', 'bbbbb', 'ccccc']
  const cmdrBroke = [0, 1]
  const wholeFile: Selection = { anchor: { row: 0, offset: 0 }, focus: { row: 2, offset: 5 } }

  it('sizes the whole thing as the 15 bytes it is, inventing no newline per row', () => {
    // 16 or 17 here would mean a delimiter was assumed between rows: on a real 300 MB
    // line that is an invented byte every 20 000, and the number feeds the 10 MB confirm
    // and the 100 MB refusal (I3).
    expect(estimateSelectionBytes(wholeFile, rowLookup(minified, cmdrBroke))).toBe(15)
  })

  it("still counts the file's OWN newlines when the rows are the file's", () => {
    // The same three strings, none of them a Cmdr break: "aaaaa\nbbbbb\nccccc" is 17.
    expect(estimateSelectionBytes(wholeFile, rowLookup(minified))).toBe(17)
  })

  it('announces the character count without a separator between continuation rows', () => {
    // 15 characters, the same as the text. A per-row newline would say 17. And all three
    // rows are one physical line, so the listener hears the line the gutter numbers, not
    // "lines 1 to 3".
    const announced = (row: number) => {
      const text = minified.at(row)
      if (text === undefined) return null
      return { utf16Length: text.length, lineNumber: cmdrBroke.includes(row - 1) ? null : row }
    }
    expect(describeSelectionForAt(wholeFile, announced)).toBe('Selected 15 characters on line 1')
  })

  it('sizes a partial selection across a Cmdr break the same way', () => {
    // From row 0 offset 2 to row 2 offset 3: "aaa" + "bbbbb" + "ccc" = 11, no delimiters.
    expect(
      estimateSelectionBytes(
        { anchor: { row: 0, offset: 2 }, focus: { row: 2, offset: 3 } },
        rowLookup(minified, cmdrBroke),
      ),
    ).toBe(11)
  })
})
