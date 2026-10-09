/**
 * Unit tests for the results table's column declarations (`result-column-widths.ts`): which
 * width policy each column takes, and what each measured column's demand reads off a row.
 * The generic split and demand math is pinned in `$lib/ui/column-list-layout.test.ts`.
 *
 * Widths are mocked (10 px per character). The real measurement is pretext's job; mocking it
 * keeps the declarations testable without a canvas.
 */

import { describe, expect, it } from 'vitest'
import type { ColumnListMeasurers } from '$lib/ui/column-list-types'
import type { SearchResultEntry } from '$lib/tauri-commands'
import { NAME_COL_MIN_PX, PATH_COL_MIN_PX, resultColumnWidths, type ResultRowTexts } from './result-column-widths'
import { PILL_CHROME_PX, PILL_SEPARATOR_GAP_PX } from './path-pills-layout'

const CHAR_PX = 10
const text = (s: string): number => s.length * CHAR_PX
// Doubles every measured digit, so a test can tell which measurer a column used.
const tabular = (s: string): number => text(s) + (s.match(/[0-9]/g)?.length ?? 0) * CHAR_PX
const measure: ColumnListMeasurers = { text, tabular }

const texts = (entry: SearchResultEntry): ResultRowTexts => ({
  name: entry.name,
  pathLabels: entry.parentPath.split('/').filter(Boolean),
  size: '12 kB',
  modified: '2026-09-29 16:25',
})

const row: SearchResultEntry = {
  path: '/Users/x/Downloads/report.pdf',
  name: 'report.pdf',
  parentPath: 'Users/x/Downloads',
  isDirectory: false,
  size: 12_000,
  modifiedAt: 0,
  iconId: 'ext:pdf',
}

describe('resultColumnWidths with a Path column (Search)', () => {
  const widths = resultColumnWidths(true, texts)

  it('shares the width between Name and Path, floored', () => {
    expect(widths.name.width).toEqual({ kind: 'share', minPx: NAME_COL_MIN_PX })
    expect(widths.path.width).toEqual({ kind: 'share', minPx: PATH_COL_MIN_PX })
  })

  it('measures Name off the entry name', () => {
    expect(widths.name.demand?.({ row, measure })).toBe(text('report.pdf'))
  })

  it('measures Path as the uncollapsed pill strip, header inset by half a pill', () => {
    const labels = ['Users', 'x', 'Downloads']
    const sep = text('/') + PILL_SEPARATOR_GAP_PX
    const strip = labels.reduce((w, l) => w + text(l) + PILL_CHROME_PX, 0) + sep * (labels.length - 1)
    expect(widths.path.demand?.({ row, measure })).toBe(strip)
    expect(widths.path.headerDemand?.({ label: 'Path', measure })).toBe(text('Path') + PILL_CHROME_PX / 2)
  })

  it('shrink-wraps Size and Modified on tabular digits, falling back to 10ch and 16ch', () => {
    expect(widths.size.width).toEqual({ kind: 'fit', fallback: { kind: 'fixed', ch: 10 } })
    expect(widths.modified.width).toEqual({ kind: 'fit', fallback: { kind: 'fixed', ch: 16 } })
    expect(widths.size.demand?.({ row, measure })).toBe(tabular('12 kB'))
    expect(widths.modified.demand?.({ row, measure })).toBe(tabular('2026-09-29 16:25'))
  })
})

describe('resultColumnWidths without a Path column (Selection)', () => {
  const widths = resultColumnWidths(false, texts)

  it('keeps Name as the lone flex track and measures nothing', () => {
    expect(widths.name.width).toEqual({ kind: 'share', minPx: NAME_COL_MIN_PX })
    expect(widths.name.demand).toBeUndefined()
    expect(widths.size.width).toEqual({ kind: 'fixed', ch: 10 })
    expect(widths.modified.width).toEqual({ kind: 'fixed', ch: 16 })
  })
})
