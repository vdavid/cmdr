import { describe, it, expect } from 'vitest'
import {
  bandSegments,
  columnPath,
  foldedColumn,
  lineRuns,
  shortDay,
  stackGap,
  type ColumnSeries,
} from './daily-chart.js'

describe('foldedColumn', () => {
  const layer = (key: string, values: (number | null)[], foldInto?: string): ColumnSeries => ({
    key,
    label: key,
    color: '#000000',
    values,
    ...(foldInto ? { foldInto } : {}),
  })

  it('moves a segment under the threshold into its fold target, keeping the column total', () => {
    const layers = [layer('US', [20]), layer('DE', [3], 'other'), layer('HU', [7], 'other'), layer('other', [5])]
    // 5% of a 150 axis is 7.5: DE (3) and HU (7) fold, US (20) stays.
    expect(foldedColumn(layers, 0, 7.5).map((s) => [s.series.key, s.value])).toEqual([
      ['US', 20],
      ['other', 15],
    ])
  })

  it('gives a fold target that had nothing of its own a segment of what it absorbed', () => {
    const layers = [layer('US', [20]), layer('DE', [2], 'other'), layer('other', [0])]
    expect(foldedColumn(layers, 0, 7.5).map((s) => [s.series.key, s.value])).toEqual([
      ['US', 20],
      ['other', 2],
    ])
  })

  it('leaves a thin segment alone when it has no fold target in the stack', () => {
    const layers = [layer('direct', [2]), layer('homebrew', [1], 'missing')]
    expect(foldedColumn(layers, 0, 7.5).map((s) => [s.series.key, s.value])).toEqual([
      ['direct', 2],
      ['homebrew', 1],
    ])
  })

  it('drops zero and unknown values', () => {
    const layers = [layer('US', [0]), layer('DE', [null], 'other'), layer('other', [4])]
    expect(foldedColumn(layers, 0, 7.5).map((s) => [s.series.key, s.value])).toEqual([['other', 4]])
  })
})

describe('stackGap', () => {
  it('keeps a thin slice at its full, honest height', () => {
    expect(stackGap(2.5)).toBe(0)
    expect(stackGap(3.9)).toBe(0)
  })

  it('separates a slice tall enough to spare a pixel, by one pixel only', () => {
    expect(stackGap(4)).toBe(1)
    expect(stackGap(80)).toBe(1)
  })
})

describe('lineRuns', () => {
  it('breaks the line at unknown days', () => {
    expect(lineRuns([1, 2, null, 3])).toEqual([
      [
        { index: 0, value: 1 },
        { index: 1, value: 2 },
      ],
      [{ index: 3, value: 3 }],
    ])
    expect(lineRuns([null, null])).toEqual([])
  })
})

describe('columnPath', () => {
  it('draws nothing for an empty column', () => {
    expect(columnPath(0, 0, 10, 0)).toBe('')
  })

  it('never rounds past half the width or the height', () => {
    // A 2px-tall column: the corner radius clamps to 2, so the path stays inside the box.
    expect(columnPath(0, 8, 10, 2)).toBe('M0,10V10Q0,8 2,8H8Q10,8 10,10V10Z')
  })
})

describe('shortDay', () => {
  it('reads the day in UTC', () => {
    expect(shortDay('2026-11-03')).toBe('Nov 3')
  })
})

describe('bandSegments', () => {
  const bands = { yellow: 0.05, green: 0.12 }

  it('splits the axis into red, yellow, and green, clipped to the axis top', () => {
    expect(bandSegments(bands, 0.2)).toEqual([
      { status: 'bad', from: 0, to: 0.05 },
      { status: 'fair', from: 0.05, to: 0.12 },
      { status: 'good', from: 0.12, to: 0.2 },
    ])
  })

  it('drops the bands above the axis top', () => {
    expect(bandSegments(bands, 0.08)).toEqual([
      { status: 'bad', from: 0, to: 0.05 },
      { status: 'fair', from: 0.05, to: 0.08 },
    ])
    expect(bandSegments(bands, 0.04)).toEqual([{ status: 'bad', from: 0, to: 0.04 }])
  })
})
