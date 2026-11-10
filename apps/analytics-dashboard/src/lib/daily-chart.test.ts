import { describe, it, expect } from 'vitest'
import { bandSegments, columnPath, lineRuns, shortDay } from './daily-chart.js'

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
