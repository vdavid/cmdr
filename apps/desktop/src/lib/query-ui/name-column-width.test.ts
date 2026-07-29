/**
 * Unit tests for the Name-column shrink-wrap math (`name-column-width.ts`).
 *
 * Widths are mocked (10 px per character, so `measure('0') === 10` and the `22ch` cap is
 * 220 px) — the real measurement is pretext's job, and mocking it keeps the algorithm
 * testable without a canvas.
 */

import { describe, expect, it } from 'vitest'
import {
  computeNameColumnWidth,
  visibleRowRange,
  NAME_COL_MIN_PX,
  NAME_MEASUREMENT_PAD,
} from './name-column-width'

const CHAR_PX = 10
const measure = (text: string): number => text.length * CHAR_PX
/** `measure('0') * 22`, the `ch`-derived ceiling with this mock font. */
const CAP = 220

describe('computeNameColumnWidth', () => {
  it('shrink-wraps to the widest visible name, not to the ceiling', () => {
    // The whole point of the change: a screenful of same-ish names must not reserve 22ch.
    const names = ['report-2026-01.pdf', 'report-2026.pdf', 'notes.md']
    const width = computeNameColumnWidth({ names, headerLabel: 'Name', measure })
    expect(width).toBe(measure('report-2026-01.pdf') + NAME_MEASUREMENT_PAD)
    expect(width).toBeLessThan(CAP)
  })

  it('ignores rows that scrolled out of view (only the passed-in names count)', () => {
    const onScreen = ['a-medium-name.txt']
    const withLongOffScreenRow = [...onScreen, 'a-really-very-extremely-long-file-name.tar.gz']
    expect(computeNameColumnWidth({ names: onScreen, headerLabel: 'Name', measure })).toBeLessThan(
      computeNameColumnWidth({ names: withLongOffScreenRow, headerLabel: 'Name', measure }),
    )
  })

  it('never drops below the floor, so a very short list still reads as a column', () => {
    expect(computeNameColumnWidth({ names: ['a'], headerLabel: 'N', measure })).toBe(NAME_COL_MIN_PX)
  })

  it('caps at 22ch so one pathological name cannot eat the Path column', () => {
    const width = computeNameColumnWidth({
      names: ['a-really-very-extremely-long-file-name.tar.gz'],
      headerLabel: 'Name',
      measure,
    })
    // Capped, which is what leaves the name mid-truncating with an ellipsis instead.
    expect(width).toBe(CAP)
  })

  it('always fits the header label, even when every name is narrower', () => {
    const width = computeNameColumnWidth({ names: ['a.txt'], headerLabel: 'Nom du fichier', measure })
    expect(width).toBeGreaterThanOrEqual(measure('Nom du fichier'))
  })

  it('lets the ceiling win over a header label wider than 22ch', () => {
    // The header ellipsizes rather than the Path column losing its width — the same
    // tie-break the fixed `22ch` track had.
    const width = computeNameColumnWidth({
      names: ['a.txt'],
      headerLabel: 'An absurdly long localized column header',
      measure,
    })
    expect(width).toBe(CAP)
  })

  it('falls back to header + floor when nothing is visible', () => {
    expect(computeNameColumnWidth({ names: [], headerLabel: 'Name', measure })).toBe(NAME_COL_MIN_PX)
  })
})

describe('visibleRowRange', () => {
  it('returns the rows the viewport actually covers', () => {
    // 100 rows of 20 px, a 100 px viewport scrolled to row 5.
    expect(visibleRowRange(100, 100, 20, 100)).toEqual({ start: 5, end: 10 })
  })

  it('includes the partially visible row at each edge', () => {
    expect(visibleRowRange(10, 100, 20, 100)).toEqual({ start: 0, end: 6 })
  })

  it('clamps to the end of the list', () => {
    expect(visibleRowRange(1_900, 100, 20, 100)).toEqual({ start: 95, end: 100 })
  })

  it('never returns an empty range for a non-empty list', () => {
    // Scrolled past the end (momentum overscroll, or a list that just shrank).
    const range = visibleRowRange(10_000, 100, 20, 3)
    expect(range.end).toBeGreaterThan(range.start)
  })

  // Degenerate geometry means "measure everything": a slightly wide column is a much
  // smaller sin than clipping names to a range we guessed wrong.
  it('falls back to the whole list when the row height is unknown', () => {
    expect(visibleRowRange(0, 100, 0, 30)).toEqual({ start: 0, end: 30 })
  })

  it('falls back to the whole list before the container has a height', () => {
    expect(visibleRowRange(0, 0, 20, 30)).toEqual({ start: 0, end: 30 })
  })

  it('handles an empty list', () => {
    expect(visibleRowRange(0, 100, 20, 0)).toEqual({ start: 0, end: 0 })
  })
})
