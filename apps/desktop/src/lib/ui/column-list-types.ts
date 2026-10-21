/**
 * `ColumnList`'s public types: what a consumer declares per column, and the two shapes of
 * data source. DETAILS.md § ColumnList has the contract in prose.
 */

import type { Snippet } from 'svelte'
import type { ColumnWidth } from './column-list-layout'
import type { ListCursorKeyOptions } from './list-cursor'

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
   * `grid`: a table that is itself a Tab stop with a cursor: it takes focus, answers the
   * cursor keys and clicks (`onCursorChange`), and names its cursor row through
   * `aria-activedescendant`, its cells read under their column headers.
   */
  semantics?: 'listbox' | 'table' | 'grid'
  ariaLabel: string
  /** The row under the cursor, or -1. */
  cursorIndex?: number
  /**
   * The list moved the cursor: a click on a data row, a cursor key in a `grid`, and the
   * pointer entering a row with `hoverMovesCursor`. Writing it to `cursorIndex` is what
   * makes them one cursor.
   */
  onCursorChange?: (index: number) => void
  /**
   * Hover moves the cursor too (Search, where the mouse and the arrows share it). Off by
   * default: a cursor that drives something else (the Multi-rename range examples) mustn't
   * jump while the pointer passes over.
   */
  hoverMovesCursor?: boolean
  /** A `grid`'s keydown that isn't one of its cursor keys (the Multi-rename preview's ⇧↑ / ⇧↓). */
  onKeydown?: (event: KeyboardEvent) => void
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
  /**
   * Listbox semantics only: the listbox's id, and the prefix of every option's
   * (`{id}-option-{index}`), so a combobox outside the list can name its cursor row
   * (`aria-controls`, and `aria-activedescendant` through `activeDescendantId`). Must be
   * document-unique.
   */
  id?: string
}

/** How `cursorTarget` reads a key. */
export interface ColumnListCursorKeyOptions extends ListCursorKeyOptions {
  /** ↓ on the last row goes to the first and ↑ on the first to the last. Pages and ends never wrap. */
  wrap?: boolean
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
  /** Row `index`'s option id while it's drawn, else `undefined` (needs the `id` prop). Reactive. */
  activeDescendantId: (index: number) => string | undefined
  /** How many rows fit on screen: what a page key moves by (less one). */
  visibleRowCount: () => number
  /**
   * Where a cursor key (↑ ↓ PageUp PageDown Home End, no modifier) takes the cursor from row
   * `from`: a data row, stepping over group headings, or -1 when no row can take it. `null`
   * for any other key, which stays the caller's.
   */
  cursorTarget: (event: KeyboardEvent, from: number, options?: ColumnListCursorKeyOptions) => number | null
}
