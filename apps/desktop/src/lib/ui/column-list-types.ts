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

/** A row as a source answers for it: `undefined` while a windowed row hasn't arrived. */
export type ColumnListRow<T> = ReturnType<ColumnListWindowedSource<T>['getRow']>

/**
 * `ColumnList`'s props. The type-aware linter can't instantiate a generic Svelte component,
 * so it reads every prop as `any` and flags each inline callback; build the props in
 * TypeScript with `columnListProps` and spread them instead.
 */
export interface ColumnListProps<T> {
  columns: ColumnListColumn<T>[]
  /** An array (measured columns walk it), or a windowed source that pages on demand. */
  rows: ColumnListSource<T>
  /** Keys an array source's rows so a re-sorted list moves rows instead of rebuilding them. */
  rowKey?: (row: T) => string | number
  /**
   * `listbox` (default): rows are options and the cursor row is `aria-selected`, for
   * lists you move through and act on. `table`: rows and cells, for previews and rows
   * holding their own controls (an option can't contain a checkbox or a text field).
   */
  semantics?: 'listbox' | 'table'
  ariaLabel: string
  /** The row under the cursor, or -1. */
  cursorIndex?: number
  /** The pointer entered a data row. Writing it to `cursorIndex` gives the single cursor. */
  onHover?: (index: number) => void
  onRowClick?: (index: number) => void
  /** Right-click on a data row. The list calls `preventDefault` when this is set. */
  onRowContextMenu?: (payload: ColumnListRowEvent<T>) => void
  /** Marks a row as a group heading (a folder name above its files). */
  isGroupHeading?: (row: T) => boolean
  /** Renders a group heading across every column. */
  groupHeading?: Snippet<[ColumnListCellContext<T>]>
  /** Consumer hook classes on the header and each row (test selectors, contrast audits). */
  headerClass?: string
  rowClass?: string
  /** A CSS length overriding the default row height, for taller rows (thumbnails). */
  rowHeight?: string
  /**
   * `true` (default): fixed-height rows, drawn in a window. `false`: every row drawn, each
   * as tall as its content (the row height becomes a floor), for short lists whose rows
   * grow (badges, a wrapped quote, a text field). Array sources only, in practice: it
   * draws the whole `count`.
   */
  virtualized?: boolean
}

/**
 * Types `ColumnList`'s props in TypeScript, inferring `T` from `rows`, so inline callbacks
 * (`rowKey: (entry) => entry.path`) are typed where the linter can see them:
 * `<ColumnList bind:this={list} {...columnListProps({ columns, rows, ... })} />`.
 */
export function columnListProps<T>(props: ColumnListProps<T>): ColumnListProps<T> {
  return props
}

/** What a `bind:this` ref to a `ColumnList` exposes (a typed ref, since the linter can't see the generic). */
export interface ColumnListApi {
  scrollIndexIntoView: (index: number) => void
}
