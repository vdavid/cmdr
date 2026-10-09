/**
 * `ColumnList`'s public types: what a consumer declares per column, and the two shapes of
 * data source. DETAILS.md § ColumnList has the contract in prose.
 */

import type { Snippet } from 'svelte'
import type { ColumnWidth } from './column-list-layout'

export type { ColumnWidth } from './column-list-layout'

/** Pixel-accurate measurers at the column's own rendered font, handed to `demand`. */
export interface ColumnListMeasurers {
  text: (text: string) => number
  /** Same, counting every digit as the widest one, for cells that render `tabular-nums`. */
  tabular: (text: string) => number
}

export interface ColumnDemandArgs<T> {
  row: T
  measure: ColumnListMeasurers
}

/** What a cell snippet receives. */
export interface ColumnListCellContext<T> {
  row: T
  index: number
  isUnderCursor: boolean
}

export interface ColumnListColumn<T> {
  /** Stable id; also the `data-column` attribute on the header label and every cell. */
  id: string
  /** Header text, and what the header demand measures. Empty means a decorative column. */
  label: string
  /** Keeps the header for screen readers only (a narrow column whose label wouldn't fit). */
  labelHidden?: boolean
  /** Renders the header label instead of the plain text, for a consumer-styled inset. */
  header?: Snippet
  width: ColumnWidth
  /**
   * The width a cell needs, in CSS pixels, from the row DATA (never from DOM text a
   * truncating action wrote). Present on `fit` columns, optional on `share` ones; only read
   * for an array source. A column without one is never measured.
   */
  demand?: (args: ColumnDemandArgs<T>) => number
  /** The header's own width, when it isn't plain `label` text at the column font. */
  headerDemand?: (args: { label: string; measure: ColumnListMeasurers }) => number
  align?: 'start' | 'end'
  /** The row's primary text: weight 500, and measured at that weight. */
  emphasis?: boolean
  /** Text color at rest. Under the cursor every tone reads at primary (AA on the accent tint). */
  tone?: 'primary' | 'secondary' | 'tertiary'
  /**
   * Clips the cell's content with an ellipsis (default). Turn it off for cells holding
   * controls, whose focus rings would otherwise be cut at the cell edge.
   */
  clip?: boolean
  /** A consumer hook class on this column's cells (test selectors, contrast audits). */
  class?: string
  cell: Snippet<[ColumnListCellContext<T>]>
}

/**
 * A data source too big to hold: the consumer knows the count, answers `getRow` from what it
 * has loaded (reactively, so a late page re-renders), and is told which rows are on screen so
 * it can fetch them. `getRow` returning `undefined` draws a placeholder row.
 */
export interface ColumnListWindowedSource<T> {
  readonly count: number
  getRow: (index: number) => T | undefined
  onRangeChange?: (range: { start: number; end: number }) => void
}

export type ColumnListSource<T> = readonly T[] | ColumnListWindowedSource<T>

export interface ColumnListRowEvent<T> {
  index: number
  row: T
  event: MouseEvent
}
