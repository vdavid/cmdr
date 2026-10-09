/**
 * Unit tests for `ColumnList`'s pure layout math (`column-list-layout.ts`): column demands,
 * the max-min-fair split of the shared width, the grid template, and the virtual window.
 *
 * Widths are plain numbers here. The real measurement is pretext's job; keeping it out keeps
 * the algorithm testable without a canvas.
 */

import { describe, expect, it } from 'vitest'
import {
  buildGridTemplate,
  columnDemand,
  createTabularMeasure,
  MEASUREMENT_PAD,
  revealScrollTop,
  splitShareColumns,
  visibleRange,
  type LayoutColumn,
} from './column-list-layout'

describe('columnDemand', () => {
  it('takes the widest cell or header, padded and rounded up', () => {
    expect(columnDemand({ header: 20, cells: [35.2, 70.4, 10], minPx: 0 })).toBe(Math.ceil(70.4 + MEASUREMENT_PAD))
    expect(columnDemand({ header: 90, cells: [10], minPx: 0 })).toBe(90 + MEASUREMENT_PAD)
  })

  it('floors the demand at the column minimum', () => {
    expect(columnDemand({ header: 10, cells: [10], minPx: 80 })).toBe(80)
  })
})

describe('createTabularMeasure', () => {
  it('measures every digit as the widest one', () => {
    // A font where "1" is narrow: the rendered tabular "11" is as wide as "88".
    const narrowOnes = (text: string): number => {
      const ones = text.length - text.replaceAll('1', '').length
      return ones * 4 + (text.length - ones) * 10
    }
    expect(createTabularMeasure(narrowOnes)('111 kB')).toBe(narrowOnes('888 kB'))
  })
})

describe('splitShareColumns', () => {
  const min = [80, 120]

  it('shows both in full when they fit, handing the spare width to the last column', () => {
    expect(splitShareColumns({ available: 1000, demands: [300, 200], minPx: min })).toEqual([
      { kind: 'fixed', px: 300 },
      { kind: 'flex', minPx: 120 },
    ])
  })

  it('keeps the first column whole when both fit even though it needs more than half', () => {
    expect(splitShareColumns({ available: 1000, demands: [700, 100], minPx: min })).toEqual([
      { kind: 'fixed', px: 700 },
      { kind: 'flex', minPx: 120 },
    ])
  })

  it('gives a short column exactly what it needs and the rest to a long one', () => {
    expect(splitShareColumns({ available: 600, demands: [200, 900], minPx: min })).toEqual([
      { kind: 'fixed', px: 200 },
      { kind: 'flex', minPx: 120 },
    ])
    expect(splitShareColumns({ available: 600, demands: [900, 200], minPx: min })).toEqual([
      { kind: 'flex', minPx: 80 },
      { kind: 'fixed', px: 200 },
    ])
  })

  it('splits evenly when every column needs more than its share', () => {
    expect(splitShareColumns({ available: 600, demands: [900, 400], minPx: min })).toEqual([
      { kind: 'flex', minPx: 80 },
      { kind: 'flex', minPx: 120 },
    ])
  })

  it('splits evenly before the container has a width', () => {
    expect(splitShareColumns({ available: 0, demands: [100, 100], minPx: min })).toEqual([
      { kind: 'flex', minPx: 80 },
      { kind: 'flex', minPx: 120 },
    ])
  })

  it('water-fills across three columns: what a short one leaves goes to the others', () => {
    // 900 / 3 = 300: the 100 fits. 800 / 2 = 400: the 350 fits. The 1,000 takes the rest.
    expect(splitShareColumns({ available: 900, demands: [1000, 100, 350], minPx: [0, 0, 0] })).toEqual([
      { kind: 'flex', minPx: 0 },
      { kind: 'fixed', px: 100 },
      { kind: 'fixed', px: 350 },
    ])
  })

  it('treats an unmeasured column as always wanting more', () => {
    expect(splitShareColumns({ available: 1000, demands: [null, 100], minPx: [0, 0] })).toEqual([
      { kind: 'flex', minPx: 0 },
      { kind: 'fixed', px: 100 },
    ])
  })
})

describe('buildGridTemplate', () => {
  const icon: LayoutColumn = { width: { kind: 'fixed', px: 24 } }
  const name: LayoutColumn = { width: { kind: 'share', minPx: 80 }, measured: true }
  const path: LayoutColumn = { width: { kind: 'share', minPx: 120 }, measured: true }
  const size: LayoutColumn = { width: { kind: 'fit', fallback: { kind: 'fixed', ch: 10 } }, measured: true }
  const modified: LayoutColumn = { width: { kind: 'fit', fallback: { kind: 'fixed', ch: 16 } }, measured: true }
  const columns = [icon, name, path, size, modified]

  it('falls back to an even split and the fit columns fallbacks before measuring', () => {
    expect(buildGridTemplate({ columns, demands: null, containerWidth: 600, rowChrome: 0, chPx: 8 })).toBe(
      '24px minmax(80px, 1fr) minmax(120px, 1fr) 10ch 16ch',
    )
  })

  it('renders an unmeasured layout without any measurement at all', () => {
    const plainName: LayoutColumn = { width: { kind: 'share', minPx: 80 } }
    const tenCh: LayoutColumn = { width: { kind: 'fixed', ch: 10 } }
    const sixteenCh: LayoutColumn = { width: { kind: 'fixed', ch: 16 } }
    expect(
      buildGridTemplate({
        columns: [icon, plainName, tenCh, sixteenCh],
        demands: null,
        containerWidth: 600,
        rowChrome: 0,
        chPx: 8,
      }),
    ).toBe('24px minmax(80px, 1fr) 10ch 16ch')
  })

  it('pins fit columns to their demand and splits what the fixed tracks leave', () => {
    // available = 700 - 40 chrome - 24 icon - 60 size - 76 modified = 500.
    // Name 150 fits in its 250 half; Path takes the rest.
    expect(
      buildGridTemplate({
        columns,
        demands: [null, 150, 900, 60, 76],
        containerWidth: 700,
        rowChrome: 40,
        chPx: 8,
      }),
    ).toBe('24px 150px minmax(120px, 1fr) 60px 76px')
  })

  it('counts a ch-sized fixed track in pixels when splitting', () => {
    const tenCh: LayoutColumn = { width: { kind: 'fixed', ch: 10 } }
    // available = 300 - 10 ch * 10 px = 200; halves of 100: Name 90 fits, Path flexes.
    expect(
      buildGridTemplate({
        columns: [name, path, tenCh],
        demands: [90, 500, null],
        containerWidth: 300,
        rowChrome: 0,
        chPx: 10,
      }),
    ).toBe('90px minmax(120px, 1fr) 10ch')
    // Without the ch track counted, available would be 300 and Name 140 would fit its half.
    expect(
      buildGridTemplate({
        columns: [name, path, tenCh],
        demands: [140, 500, null],
        containerWidth: 300,
        rowChrome: 0,
        chPx: 10,
      }),
    ).toBe('minmax(80px, 1fr) minmax(120px, 1fr) 10ch')
  })
})

describe('visibleRange', () => {
  it('draws the rows in view plus the overscan on both sides', () => {
    expect(visibleRange({ count: 1000, rowHeight: 20, scrollTop: 2000, viewportHeight: 200, overscan: 5 })).toEqual({
      start: 95,
      end: 115,
    })
  })

  it('clamps to the list ends', () => {
    expect(visibleRange({ count: 12, rowHeight: 20, scrollTop: 0, viewportHeight: 200, overscan: 5 })).toEqual({
      start: 0,
      end: 12,
    })
    expect(visibleRange({ count: 100, rowHeight: 20, scrollTop: 1800, viewportHeight: 200, overscan: 5 })).toEqual({
      start: 85,
      end: 100,
    })
  })

  it('draws a fixed head of the list before the layout is known', () => {
    expect(visibleRange({ count: 200_000, rowHeight: 0, scrollTop: 0, viewportHeight: 0, overscan: 5 })).toEqual({
      start: 0,
      end: 100,
    })
    expect(visibleRange({ count: 30, rowHeight: 20, scrollTop: 0, viewportHeight: 0, overscan: 5 })).toEqual({
      start: 0,
      end: 30,
    })
  })

  it('stays inside the list when it shrinks under a deep scroll', () => {
    expect(visibleRange({ count: 10, rowHeight: 20, scrollTop: 5000, viewportHeight: 200, overscan: 5 })).toEqual({
      start: 10,
      end: 10,
    })
  })
})

describe('revealScrollTop', () => {
  const base = { rowHeight: 20, viewportHeight: 100 }

  it('leaves a fully visible row alone', () => {
    expect(revealScrollTop({ ...base, index: 3, scrollTop: 0 })).toBeUndefined()
  })

  it('aligns a row above the viewport to its top edge', () => {
    expect(revealScrollTop({ ...base, index: 2, scrollTop: 100 })).toBe(40)
  })

  it('aligns a row below the viewport to its bottom edge', () => {
    expect(revealScrollTop({ ...base, index: 9, scrollTop: 0 })).toBe(100)
  })
})
