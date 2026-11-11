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
  MAX_SCROLL_HEIGHT_PX,
  MEASUREMENT_PAD,
  REACH_END_ROWS,
  reachesEnd,
  revealScrollTop,
  scrollMapping,
  splitShareColumns,
  visibleRange,
  windowPlacement,
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

describe('reachesEnd', () => {
  const at = (scrollTop: number, count = 100) => reachesEnd({ count, rowHeight: 20, scrollTop, viewportHeight: 200 })

  it('holds once the last row in view is within REACH_END_ROWS of the end', () => {
    // Rows 0–9 in view of 100: far from the end.
    expect(at(0)).toBe(false)
    // Last row in view is 100 - REACH_END_ROWS.
    expect(at((90 - REACH_END_ROWS) * 20)).toBe(true)
    expect(at((89 - REACH_END_ROWS) * 20)).toBe(false)
  })

  it('holds for a list that fits whole, and never for an empty one', () => {
    expect(at(0, 5)).toBe(true)
    expect(at(0, 0)).toBe(false)
  })

  it('says nothing before the list has a layout', () => {
    expect(reachesEnd({ count: 5, rowHeight: 0, scrollTop: 0, viewportHeight: 200 })).toBe(false)
    expect(reachesEnd({ count: 5, rowHeight: 20, scrollTop: 0, viewportHeight: 0 })).toBe(false)
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

// Past WebKit's layout-height cap the list scrolls a scaled position (`MAX_SCROLL_HEIGHT_PX`):
// 2,000,000 rows of 20 px are 40M virtual pixels in a 32M-pixel scroller.
describe('scrollMapping and windowPlacement', () => {
  const rowHeight = 20
  const viewportHeight = 200
  const count = 2_000_000
  const mapping = scrollMapping({ count, rowHeight, viewportHeight })

  function placementAt(scrollTop: number) {
    const range = visibleRange({
      count,
      rowHeight,
      scrollTop: scrollTop * mapping.ratio,
      viewportHeight,
      overscan: 20,
    })
    return { range, ...windowPlacement({ count, range, rowHeight, scrollTop, mapping }) }
  }

  /** Where row `index` shows, in pixels below the viewport's top, for a scroll at `scrollTop`. */
  function shownAt(index: number, scrollTop: number): number {
    const { range, padTop, marginTop } = placementAt(scrollTop)
    expect(index).toBeGreaterThanOrEqual(range.start)
    expect(index).toBeLessThan(range.end)
    return marginTop + padTop + (index - range.start) * rowHeight - scrollTop
  }

  it('lays out every pixel while the rows fit under the cap', () => {
    expect(scrollMapping({ count: 1_000, rowHeight, viewportHeight })).toEqual({ contentHeight: 20_000, ratio: 1 })
    expect(
      windowPlacement({
        count: 1_000,
        range: { start: 80, end: 130 },
        rowHeight,
        scrollTop: 2_000,
        mapping: { contentHeight: 20_000, ratio: 1 },
      }),
    ).toEqual({ padTop: 1_600, padBottom: 20_000 - 2_600, marginTop: 0 })
  })

  it('caps the content past the limit and scales the scroll so the last position shows the last row', () => {
    expect(mapping.contentHeight).toBe(MAX_SCROLL_HEIGHT_PX)
    expect(mapping.ratio).toBeCloseTo((40_000_000 - 200) / (MAX_SCROLL_HEIGHT_PX - 200))
    // The last row's bottom edge meets the viewport's bottom edge.
    expect(shownAt(count - 1, MAX_SCROLL_HEIGHT_PX - viewportHeight)).toBeCloseTo(viewportHeight - rowHeight, 1)
  })

  it('keeps each drawn row at its virtual position, inside a content of fixed height', () => {
    for (const scrollTop of [0, 7, 150, 1_234_567, 20_000_000, MAX_SCROLL_HEIGHT_PX - viewportHeight]) {
      const virtualTop = scrollTop * mapping.ratio
      const first = Math.floor(virtualTop / rowHeight)
      expect(shownAt(first, scrollTop)).toBeCloseTo(first * rowHeight - virtualTop, 1)
      const { range, padTop, padBottom, marginTop } = placementAt(scrollTop)
      expect(padTop).toBeGreaterThanOrEqual(0)
      expect(padBottom).toBeGreaterThanOrEqual(0)
      expect(marginTop).toBeLessThanOrEqual(0)
      const height = marginTop + padTop + (range.end - range.start) * rowHeight + padBottom
      expect(height).toBeCloseTo(MAX_SCROLL_HEIGHT_PX, 1)
      // A tiny float written as `1.4e-8px` is invalid CSS, and the old spacer would stay.
      for (const px of [padTop, padBottom, marginTop]) expect(String(px)).not.toContain('e')
    }
  })

  it('pulls the window above the content top with a margin once the first rows scroll out', () => {
    // 150 scrolled pixels are ~187 virtual ones: row 0 sits ~187 px above the viewport, so
    // ~37 px above the content's own top, which only a negative margin can place.
    const { range, padTop, marginTop } = placementAt(150)
    expect(range.start).toBe(0)
    expect(padTop).toBe(0)
    expect(marginTop).toBeCloseTo(150 - 150 * mapping.ratio, 1)
  })

  it('reveals a row by its virtual position and answers a scaled scroll position', () => {
    const ratio = mapping.ratio
    const last = revealScrollTop({ index: count - 1, rowHeight, scrollTop: 0, viewportHeight, ratio })
    expect(last).toBeCloseTo(MAX_SCROLL_HEIGHT_PX - viewportHeight, 3)
    const middle = revealScrollTop({ index: 1_000_000, rowHeight, scrollTop: 0, viewportHeight, ratio })
    expect((middle ?? 0) * ratio).toBeCloseTo(1_000_001 * rowHeight - viewportHeight, 3)
  })
})
