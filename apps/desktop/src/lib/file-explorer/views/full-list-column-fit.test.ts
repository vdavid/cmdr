/**
 * Which optional Full-view columns fit a pane: the date drops first, then Ext,
 * so the Name column keeps `FULL_NAME_MIN_WIDTH` (scaled with text size). Size
 * always stays. The widths below are the real app's at 100 % text size (icon
 * 16, Ext ~40, Size ~70, date ~95), measured in the demo build.
 */
import { describe, it, expect } from 'vitest'
import { fitFullListColumns, FULL_NAME_MIN_WIDTH } from './full-list-column-fit'

const typical = { ext: 40, size: 70, date: 95 }

const fit = (rowAreaWidth: number, overrides: Partial<Parameters<typeof fitFullListColumns>[0]> = {}) =>
  fitFullListColumns({ rowAreaWidth, iconWidth: 16, gitWidth: 0, widths: typical, scale: 1, ...overrides })

describe('fitFullListColumns', () => {
  it('keeps every column in a wide pane', () => {
    expect(fit(800)).toEqual({ showDate: true, showExt: true })
    // The 950 px window split 50/50: today's layout, untouched.
    expect(fit(470)).toEqual({ showDate: true, showExt: true })
  })

  it('drops the date first as the pane narrows', () => {
    expect(fit(320)).toEqual({ showDate: false, showExt: true })
  })

  it('then drops Ext, leaving icon, Name, and Size', () => {
    expect(fit(240)).toEqual({ showDate: false, showExt: false })
  })

  it('drops a column exactly when Name would fall under its minimum', () => {
    // chrome 24 + icon 16 + ext 40 + size 70 + date 95 + four 8 px gaps = 277, plus Name's minimum.
    const allFit = 277 + FULL_NAME_MIN_WIDTH
    expect(fit(allFit)).toEqual({ showDate: true, showExt: true })
    expect(fit(allFit - 1).showDate).toBe(false)
  })

  it('counts the Git column against the Name width', () => {
    const allFit = 277 + FULL_NAME_MIN_WIDTH
    expect(fit(allFit, { gitWidth: 28 }).showDate).toBe(false)
  })

  it('scales the Name minimum with text size', () => {
    const at150 = { ext: 60, size: 105, date: 143 }
    expect(fit(470, { iconWidth: 24, widths: at150, scale: 1.5 }).showDate).toBe(false)
    expect(fit(800, { iconWidth: 24, widths: at150, scale: 1.5 })).toEqual({ showDate: true, showExt: true })
  })

  it('has no Ext to drop when the extension already rides in Name', () => {
    expect(fit(240, { widths: { ...typical, ext: 0 } })).toEqual({ showDate: false, showExt: true })
  })

  it('keeps the full layout while the pane is unmeasured', () => {
    expect(fit(0)).toEqual({ showDate: true, showExt: true })
  })
})
