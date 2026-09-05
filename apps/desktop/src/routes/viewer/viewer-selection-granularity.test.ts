/**
 * The pure granularity model: snapping a caret to a word or line range, and taking the
 * union of two such ranges with the drag's direction preserved.
 *
 * The pointer drag (`viewer-pointer-drag.svelte.test.ts`) covers the wiring; here the
 * geometry is gone and only the range arithmetic is left.
 */
import { describe, it, expect } from 'vitest'

import { rangeAtCaret, extendRangeToGranularity } from './viewer-selection-granularity'

const LINES = ['alpha beta gamma', 'second line']
const getLineText = (line: number): string | undefined => LINES[line]

describe('rangeAtCaret', () => {
  it('collapses to the caret at character granularity', () => {
    expect(rangeAtCaret({ caret: { line: 0, offset: 7 }, granularity: 'character', getLineText })).toEqual({
      line: 0,
      start: 7,
      end: 7,
    })
  })

  it('snaps to the surrounding word', () => {
    expect(rangeAtCaret({ caret: { line: 0, offset: 7 }, granularity: 'word', getLineText })).toEqual({
      line: 0,
      start: 6,
      end: 10,
    })
  })

  it('spans the whole logical line', () => {
    expect(rangeAtCaret({ caret: { line: 1, offset: 3 }, granularity: 'line', getLineText })).toEqual({
      line: 1,
      start: 0,
      end: 11,
    })
  })

  it('collapses on a line that has not been fetched yet', () => {
    // Mid-autoscroll the line cache runs dry. The row renders empty anyway, so a
    // collapsed range there matches what the user sees.
    expect(rangeAtCaret({ caret: { line: 9, offset: 4 }, granularity: 'word', getLineText })).toEqual({
      line: 9,
      start: 0,
      end: 0,
    })
  })
})

describe('extendRangeToGranularity', () => {
  const anchorRange = { line: 0, start: 6, end: 10 }

  it('runs from the anchor range start to the focus word end going forward', () => {
    expect(
      extendRangeToGranularity({ anchorRange, focus: { line: 0, offset: 12 }, granularity: 'word', getLineText }),
    ).toEqual({ anchor: { line: 0, offset: 6 }, focus: { line: 0, offset: 16 } })
  })

  it('runs from the anchor range end to the focus word start going backward', () => {
    expect(
      extendRangeToGranularity({ anchorRange, focus: { line: 0, offset: 2 }, granularity: 'word', getLineText }),
    ).toEqual({ anchor: { line: 0, offset: 10 }, focus: { line: 0, offset: 0 } })
  })

  it('reproduces the anchor range when the focus stays inside it', () => {
    expect(
      extendRangeToGranularity({ anchorRange, focus: { line: 0, offset: 8 }, granularity: 'word', getLineText }),
    ).toEqual({ anchor: { line: 0, offset: 6 }, focus: { line: 0, offset: 10 } })
  })

  it('takes whole lines down the file at line granularity', () => {
    expect(
      extendRangeToGranularity({
        anchorRange: { line: 0, start: 0, end: 16 },
        focus: { line: 1, offset: 4 },
        granularity: 'line',
        getLineText,
      }),
    ).toEqual({ anchor: { line: 0, offset: 0 }, focus: { line: 1, offset: 11 } })
  })

  it('reverses across lines too', () => {
    expect(
      extendRangeToGranularity({
        anchorRange: { line: 1, start: 0, end: 11 },
        focus: { line: 0, offset: 4 },
        granularity: 'line',
        getLineText,
      }),
    ).toEqual({ anchor: { line: 1, offset: 11 }, focus: { line: 0, offset: 0 } })
  })
})
