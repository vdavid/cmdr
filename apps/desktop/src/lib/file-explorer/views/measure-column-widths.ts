/**
 * Shrink-wraps the Ext / Size / Modified columns in FullList to the narrowest width
 * that fits the currently loaded entries plus the header label. Uses `@chenglou/pretext`
 * for pixel-accurate text measurement without DOM reflow.
 *
 * Only measures entries already cached on the client — for 500+ file directories
 * the width refines as more of the prefetch buffer streams in.
 */

import * as pretext from '@chenglou/pretext'

import { formatSizeTriads } from '../selection/selection-info-utils'
import type { FileEntry, SortColumn } from '../types'
import { createPretextMeasure } from '$lib/utils/shorten-middle'

import type { DirStats } from './file-list-utils'
import { getDisplayExtension, getDisplaySize, hasSizeMismatch } from './full-list-utils'

export interface ColumnWidths {
  ext: number
  size: number
  date: number
}

/**
 * CSS `font` shorthand matching `.col-date`, `.col-size`, `.col-ext` when rendered
 * with `var(--font-system)` at `var(--font-size-sm)` = 12px. Must stay in sync with
 * `apps/desktop/src/app.css` — pretext's README warns `system-ui` is unsafe for
 * layout accuracy, so we lead with `-apple-system` which resolves on macOS.
 */
const FONT = '12px -apple-system, BlinkMacSystemFont, sans-serif'

/**
 * Header overhead inside `SortableHeader` for the column currently being sorted:
 * 4px padding left + 4px padding right + 4px flex gap + 8px caret.
 */
const HEADER_CHROME_ACTIVE = 20

/**
 * Header overhead for a column that isn't being sorted: the caret is `display: none`,
 * which collapses both the glyph and the flex gap. Only the button's own padding remains.
 */
const HEADER_CHROME_INACTIVE = 8

/** Extra px added to the final column width. Zero by design — `Math.ceil` already
 *  rounds up, and the pretext measurement matches the browser's own text layout. */
const WIDTH_PADDING = 0

/** 12px icon + 2px (--spacing-xxs) left margin = 14px per size-column indicator. */
const SIZE_ICON_WIDTH = 14

/** Floor widths so empty or near-empty directories don't collapse the column to zero. */
const MIN_EXT_WIDTH = 28
const MIN_SIZE_WIDTH = 40
const MIN_DATE_WIDTH = 70

let measureWidthCached: ((text: string) => number) | null = null
let measureUnavailable = false

function getMeasure(): ((text: string) => number) | null {
  if (measureWidthCached) return measureWidthCached
  if (measureUnavailable) return null
  if (typeof document === 'undefined') return null
  // Guard the first call: pretext relies on Canvas 2D, which jsdom doesn't implement.
  // If that's the environment (unit tests rendering FullList), fall back to floor widths
  // forever instead of throwing on every render.
  try {
    const candidate = createPretextMeasure(FONT, pretext)
    candidate('probe')
    measureWidthCached = candidate
    return measureWidthCached
  } catch {
    measureUnavailable = true
    return null
  }
}

/** Exposed for tests to inject a fake measurer. */
export function _setMeasureForTests(fn: ((text: string) => number) | null): void {
  measureWidthCached = fn
  measureUnavailable = false
}

/** Concatenated triad string as it appears in the DOM (with U+2009 thin-space separators). */
function triadsText(bytes: number): string {
  return formatSizeTriads(bytes)
    .map((t) => t.value)
    .join('')
}

function sizeTextForEntry(
  entry: FileEntry,
  sizeDisplayMode: 'smart' | 'logical' | 'physical',
  indexing: boolean,
): string {
  if (entry.isDirectory) {
    const s = getDisplaySize(entry.recursiveSize, entry.recursivePhysicalSize, sizeDisplayMode)
    if (s !== undefined) return triadsText(s)
    return indexing ? 'Scanning...' : '<dir>'
  }
  const s = getDisplaySize(entry.size, entry.physicalSize, sizeDisplayMode)
  return s !== undefined ? triadsText(s) : ''
}

/** Pixel width of the size-column icons that follow the text for this row. */
function sizeIconSuffixForEntry(entry: FileEntry, indexing: boolean, showSizeMismatchWarning: boolean): number {
  let suffix = 0
  if (entry.isDirectory && indexing && entry.recursiveSize !== undefined) suffix += SIZE_ICON_WIDTH
  if (showSizeMismatchWarning) {
    const logical = entry.isDirectory ? entry.recursiveSize : entry.size
    const physical = entry.isDirectory ? entry.recursivePhysicalSize : entry.physicalSize
    if (hasSizeMismatch(logical, physical)) suffix += SIZE_ICON_WIDTH
  }
  return suffix
}

/**
 * Compute the shrink-wrapped Ext / Size / Modified column widths for the FullList.
 * Measures only `entries` plus the optional `parentDirStats` (shown on the ".." row) —
 * fully client-side, no disk/IPC calls.
 */
export function computeFullListColumnWidths(args: {
  entries: FileEntry[]
  parentDirStats?: DirStats | null
  formatDateTime: (timestamp: number) => string
  sizeDisplayMode: 'smart' | 'logical' | 'physical'
  indexing: boolean
  showSizeMismatchWarning: boolean
  sortBy: SortColumn
}): ColumnWidths {
  const { entries, parentDirStats, formatDateTime, sizeDisplayMode, indexing, showSizeMismatchWarning, sortBy } = args

  const measure = getMeasure()
  if (!measure) {
    return { ext: MIN_EXT_WIDTH, size: MIN_SIZE_WIDTH, date: MIN_DATE_WIDTH }
  }

  const chromeFor = (column: SortColumn): number => (sortBy === column ? HEADER_CHROME_ACTIVE : HEADER_CHROME_INACTIVE)

  // Start with header widths (the column must fit its header regardless of data).
  let extMax = measure('Ext') + chromeFor('extension')
  let sizeMax = measure('Size') + chromeFor('size')
  let dateMax = measure('Modified') + chromeFor('modified')

  // Per-row icons in the size column live to the right of the text; count the
  // widest icon suffix we've seen so we can add it to the data width.
  let sizeIconSuffixMax = 0

  for (const entry of entries) {
    const ext = getDisplayExtension(entry.name, entry.isDirectory)
    if (ext) {
      const w = measure(ext)
      if (w > extMax) extMax = w
    }

    const sizeText = sizeTextForEntry(entry, sizeDisplayMode, indexing)
    const iconSuffix = sizeIconSuffixForEntry(entry, indexing, showSizeMismatchWarning)
    const rowSize = (sizeText ? measure(sizeText) : 0) + iconSuffix
    if (rowSize > sizeMax) sizeMax = rowSize
    if (iconSuffix > sizeIconSuffixMax) sizeIconSuffixMax = iconSuffix

    if (entry.modifiedAt !== undefined) {
      const w = measure(formatDateTime(entry.modifiedAt))
      if (w > dateMax) dateMax = w
    }
  }

  // The ".." row borrows the current folder's recursive size — often the largest
  // number in the listing, so fold it in or the column snaps wider the moment it loads.
  if (parentDirStats) {
    const s = getDisplaySize(parentDirStats.recursiveSize, parentDirStats.recursivePhysicalSize, sizeDisplayMode)
    if (s !== undefined) {
      const w = measure(triadsText(s)) + sizeIconSuffixMax
      if (w > sizeMax) sizeMax = w
    }
  }

  return {
    ext: Math.max(MIN_EXT_WIDTH, Math.ceil(extMax + WIDTH_PADDING)),
    size: Math.max(MIN_SIZE_WIDTH, Math.ceil(sizeMax + WIDTH_PADDING)),
    date: Math.max(MIN_DATE_WIDTH, Math.ceil(dateMax + WIDTH_PADDING)),
  }
}
