/**
 * `ColumnList`'s grid template, live: measures the columns that declare a `demand`, reads the
 * fonts and row chrome off a real row, and hands back ONE `grid-template-columns` string for
 * the header and every row. The math is `column-list-layout.ts`; this factory owns the DOM
 * reads.
 *
 * Why it can't oscillate: every input (the row DATA, the cells' computed fonts, the row's
 * padding and `column-gap`, the viewport's width) comes from CSS or the dialog, never from
 * the tracks it writes. It measures data through each column's `demand`, never DOM text a
 * truncating action wrote into a cell. DETAILS.md § ColumnList.
 *
 * Call it during component init: it registers `$effect`s.
 */

import { createPretextMeasure } from '$lib/utils/shorten-middle'
import { buildGridTemplate, columnDemand, createTabularMeasure, type LayoutColumn } from './column-list-layout'
import type { ColumnListColumn, ColumnListMeasurers } from './column-list-types'

/**
 * Rows a measured column walks. Measuring is O(rows) per data change, so a long array
 * measures its head; the columns still stay put while scrolling, since the head is fixed.
 */
export const MEASURE_ROW_CAP = 2_000

export interface ColumnTracksInputs<T> {
  columns: ColumnListColumn<T>[]
  /** The rows to measure: an array source's rows, or `null` for a windowed one. */
  rows: readonly T[] | null
  /** The scrolling viewport every row fills; `undefined` until mounted. */
  viewport: HTMLElement | undefined
  /** The viewport's `clientWidth` (scrollbar excluded). */
  containerWidth: number
  /** Bumped whenever the drawn rows change, so fonts are read once a row exists. */
  drawnRows: number
}

export interface ColumnTracks {
  readonly gridTemplate: string
  /** Off for the first measured layout, so opening a dialog doesn't animate the columns in. */
  readonly animateTracks: boolean
}

function readFont(node: HTMLElement): string {
  const style = getComputedStyle(node)
  return style.font || `${style.fontSize} ${style.fontFamily}`
}

function isMeasured<T>(column: ColumnListColumn<T>): boolean {
  return column.demand !== undefined && column.width.kind !== 'fixed'
}

export function createColumnTracks<T>(inputs: () => ColumnTracksInputs<T>): ColumnTracks {
  /** A row's horizontal padding plus its column gaps, read off a real row. */
  let rowChrome = $state(0)
  /** Pixel-accurate measurer at the emphasis cells' font; null until pretext resolves. */
  let measureStrong = $state<((text: string) => number) | null>(null)
  /** Same, at the regular font the other cells share. */
  let measureRegular = $state<((text: string) => number) | null>(null)
  let animateTracks = $state(false)
  /** The fonts the current measurers were built for; a change (text size) rebuilds them. */
  let measuredFonts = ''
  let firstLayoutApplied = false

  const layoutColumns = $derived<LayoutColumn[]>(
    inputs().columns.map((column) => ({ width: column.width, measured: isMeasured(column) })),
  )

  /** Each column's demand (`null` where it has none), or `null` while nothing is measured. */
  const demands = $derived.by<(number | null)[] | null>(() => {
    const { columns, rows } = inputs()
    if (!rows || rows.length === 0 || !measureStrong || !measureRegular) return null
    if (!columns.some(isMeasured)) return null
    const head = rows.length > MEASURE_ROW_CAP ? rows.slice(0, MEASURE_ROW_CAP) : rows
    const strong: ColumnListMeasurers = { text: measureStrong, tabular: createTabularMeasure(measureStrong) }
    const regular: ColumnListMeasurers = { text: measureRegular, tabular: createTabularMeasure(measureRegular) }
    return columns.map((column) => {
      const { demand } = column
      if (!demand || !isMeasured(column)) return null
      const measure = column.emphasis ? strong : regular
      return columnDemand({
        header: column.headerDemand
          ? column.headerDemand({ label: column.label, measure })
          : measure.text(column.label),
        cells: head.map((row) => demand({ row, measure })),
        minPx: column.width.kind === 'share' ? column.width.minPx : 0,
      })
    })
  })

  const gridTemplate = $derived(
    buildGridTemplate({
      columns: layoutColumns,
      demands,
      containerWidth: inputs().containerWidth,
      rowChrome,
      chPx: measureRegular ? measureRegular('0') : 0,
    }),
  )

  /**
   * Builds (or rebuilds) both measurers from real rendered cells' fonts. Keying on the
   * computed font strings means a text-size change re-measures on its own.
   */
  async function ensureMeasurers(strongEl: HTMLElement, regularEl: HTMLElement): Promise<void> {
    const strongFont = readFont(strongEl)
    const regularFont = readFont(regularEl)
    const fonts = `${strongFont}|${regularFont}`
    if (fonts === measuredFonts) return
    // Remember the attempt (success or failure) so we don't retry per render, and drop
    // the old measurers: they were built for fonts we're no longer rendering.
    measuredFonts = fonts
    measureStrong = null
    measureRegular = null
    try {
      const pretext = await import('@chenglou/pretext')
      const strongCandidate = createPretextMeasure(strongFont, pretext)
      const regularCandidate = createPretextMeasure(regularFont, pretext)
      // Probe before adopting: pretext needs Canvas 2D and only fails on first use.
      strongCandidate('0')
      regularCandidate('0')
      measureStrong = strongCandidate
      measureRegular = regularCandidate
    } catch {
      // No canvas, or the chunk failed to load: stay on the unmeasured fallback template
      // rather than throwing on every render.
    }
  }

  // Reads the fonts and the row chrome off the first drawn data row.
  $effect(() => {
    const { viewport, columns, rows, drawnRows } = inputs()
    if (!viewport || !rows || drawnRows === 0 || !columns.some(isMeasured)) return
    const rowEl = viewport.querySelector<HTMLElement>('.column-list-row.is-data')
    if (!rowEl) return
    const cells = [...rowEl.querySelectorAll<HTMLElement>(':scope > .column-list-cell')]
    const measuredCells = cells.filter((_, i) => isMeasured(columns[i]))
    const strongEl = measuredCells.find((el) => el.classList.contains('is-emphasis'))
    const regularEl = measuredCells.find((el) => !el.classList.contains('is-emphasis'))
    const anyEl = strongEl ?? regularEl
    if (!anyEl) return
    const style = getComputedStyle(rowEl)
    const gap = parseFloat(style.columnGap) || 0
    rowChrome =
      (parseFloat(style.paddingLeft) || 0) +
      (parseFloat(style.paddingRight) || 0) +
      Math.max(0, columns.length - 1) * gap
    void ensureMeasurers(strongEl ?? anyEl, regularEl ?? anyEl)
  })

  // Turns the track transition on after the first measured layout has painted.
  $effect(() => {
    if (!demands || firstLayoutApplied) return
    firstLayoutApplied = true
    requestAnimationFrame(() => {
      animateTracks = true
    })
  })

  return {
    get gridTemplate() {
      return gridTemplate
    },
    get animateTracks() {
      return animateTracks
    },
  }
}
