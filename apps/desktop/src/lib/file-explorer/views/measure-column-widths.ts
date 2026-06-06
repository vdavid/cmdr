/**
 * Shrink-wraps the Ext / Size / Modified columns in FullList to the narrowest width
 * that fits the currently loaded entries plus the header label. Uses `@chenglou/pretext`
 * for pixel-accurate text measurement without DOM reflow.
 *
 * Only measures entries already cached on the client. For 500+ file directories
 * the width refines as more of the prefetch buffer streams in.
 */

import * as pretext from '@chenglou/pretext'

import { formatSizeForDisplay } from '../selection/selection-info-utils'
import type { FileEntry, SortColumn } from '../types'
import type { FileSizeFormat, FileSizeUnit } from '$lib/settings/types'
import { joinSegments, type FormattedDate } from '$lib/settings/format-utils'
import { createPretextMeasure } from '$lib/utils/shorten-middle'
import { getEffectiveScale, onDebouncedScaleChange } from '$lib/text-size.svelte'

import type { DirStats } from './file-list-utils'
import { getDirSizeDisplayState, getDisplayExtension, getDisplaySize, hasSizeMismatch } from './full-list-utils'

export interface ColumnWidths {
  ext: number
  size: number
  /** Total date column width (including the inter-half gap when split). */
  date: number
  /**
   * Pixel width of the left half of split date cells. Used as the inline-block
   * width of `.date-left` so the right halves (typically the time) line up
   * across rows. Zero when no visible row produces a `|` split.
   */
  dateLeft: number
}

/** Base font size of the file-list columns at scale 1, in CSS pixels. */
const BASE_FONT_PX = 12

/**
 * Builds the CSS `font` shorthand for `.col-date`, `.col-size`, `.col-ext`
 * scaled by the current effective text size. Pretext's README warns
 * `system-ui` is unsafe for layout accuracy, so we lead with `-apple-system`
 * which resolves on macOS.
 */
function buildFont(scale: number): string {
  const px = Math.max(1, Math.round(BASE_FONT_PX * scale))
  return `${String(px)}px -apple-system, BlinkMacSystemFont, sans-serif`
}

/**
 * Header overhead inside `SortableHeader` for the column currently being sorted:
 * 4px flex gap + 8px caret. The button's 4px horizontal padding is canceled
 * by an equal negative horizontal margin, so the label lines up with the data
 * cells below: only the gap+caret count toward the column track width.
 */
const HEADER_CHROME_ACTIVE = 12

/**
 * Header overhead for a column that isn't being sorted: the caret is
 * `display: none`, which collapses both the glyph and the flex gap. The
 * button's padding is offset by the negative margin, so the label is flush
 * against the track edges and chrome is zero.
 */
const HEADER_CHROME_INACTIVE = 0

/**
 * Per-cell measurement safety pad in CSS pixels.
 *
 * Pretext measures via canvas, the file list renders via DOM. On macOS
 * WKWebView the two paths can disagree on glyph advance widths by a fraction
 * of a pixel even with the same `-apple-system` font and the same integer
 * pixel size: canvas returns CoreText's typographic advance, the DOM lays
 * out with end-side antialiasing bleed and inline-block subpixel rounding.
 * The gap is invisible on most strings but accumulates on certain digit
 * combinations (a `00:NN` time renders ~1 px wider than pretext measures),
 * causing the Modified column to ellipsize for one or two rows while every
 * other entry in the same listing fits perfectly. 2 px covers the worst case
 * we've seen with the iso `YYYY-MM-DD | HH:mm` format and is small enough
 * to be invisible in the overall layout.
 */
const MEASUREMENT_SAFETY_PAD = 2

/** 12px icon + 2px (--spacing-xxs) left margin = 14px per size-column indicator. */
const SIZE_ICON_WIDTH = 14

/** Floor widths so empty or near-empty directories don't collapse the column to zero. */
const MIN_EXT_WIDTH = 28
const MIN_SIZE_WIDTH = 40
const MIN_DATE_WIDTH = 70

/**
 * Visual gap between the date and time halves of a split date cell.
 * `var(--spacing-xs)` (4px), set as `margin-left` on `.date-right` in
 * `FullList.svelte`. Mirror this value if the CSS changes, or split-date
 * columns will be one or two pixels off.
 */
const DATE_PARTS_GAP = 4

/**
 * Cap-width sample for the Ext column. A pathological extension like
 * `extension-extension-extension-…` would otherwise stretch the column wide
 * enough to push the rest of the row off-screen. The sample is measured with
 * pretext at the current font, so the cap scales with font size/family.
 * `extensionxx` ≈ 11 chars, generous enough for real-world long extensions
 * (`controller`, `component`) without truncating, and just over the literal
 * word "extension" so that word itself never gets clipped.
 */
const EXT_CAP_SAMPLE = 'extensionxx'

let measureWidthCached: ((text: string) => number) | null = null
let measureUnavailable = false
let cachedScale = 0

/**
 * Subscribe once to "settled" scale changes and invalidate the pretext
 * measurer so the next `getMeasure` call rebuilds it with the new font size.
 *
 * Why "debounced" rather than reactive: pretext rebuilds are fast but the
 * follow-up column-width recomputes aren't free, and we don't want to thrash
 * during slider drag. The CSS layer reflows immediately via `--text-scale`;
 * this path catches up after the user releases.
 */
if (typeof window !== 'undefined') {
  onDebouncedScaleChange(() => {
    measureWidthCached = null
    measureUnavailable = false
    cachedScale = 0
  })
}

function getMeasure(): ((text: string) => number) | null {
  const scale = getEffectiveScale()
  if (measureWidthCached && scale === cachedScale) return measureWidthCached
  if (measureUnavailable && scale === cachedScale) return null
  if (typeof document === 'undefined') return null
  // Guard the first call: pretext relies on Canvas 2D, which jsdom doesn't implement.
  // If that's the environment (unit tests rendering FullList), fall back to floor widths
  // forever instead of throwing on every render.
  try {
    const candidate = createPretextMeasure(buildFont(scale), pretext)
    candidate('probe')
    measureWidthCached = candidate
    cachedScale = scale
    measureUnavailable = false
    return measureWidthCached
  } catch {
    measureUnavailable = true
    cachedScale = scale
    return null
  }
}

/** Exposed for tests to inject a fake measurer. */
export function _setMeasureForTests(fn: ((text: string) => number) | null): void {
  measureWidthCached = fn
  measureUnavailable = false
}

export interface SizeFormatOpts {
  unit: FileSizeUnit
  format: FileSizeFormat
}

/**
 * The full size-cell string as it appears in the DOM. In raw-bytes mode this
 * is the concatenation of the triad chunks (with their U+2009 thin-space
 * separators); in human-friendly mode it's something like "1.02 MB".
 */
function sizeCellText(bytes: number, opts: SizeFormatOpts): string {
  return formatSizeForDisplay(bytes, opts)
    .map((t) => t.value)
    .join('')
}

function sizeTextForEntry(
  entry: FileEntry,
  sizeDisplayMode: 'smart' | 'logical' | 'physical',
  sizeFormatOpts: SizeFormatOpts,
  isRestricted: boolean,
): string {
  // TCC-restricted entries render `<no perms>` instead of the misleading `0`
  // the indexer recorded after a denied scan. Keep this BEFORE the
  // `entry.displaySize` check: restricted state takes priority over virtual
  // git display strings (which wouldn't apply to favorites anyway).
  if (isRestricted) return '<no perms>'
  // Virtual git entries override the Size cell with a short string
  // (`+12 / -3`, `5 files`, …); measure that instead of the byte format.
  if (entry.displaySize != null) {
    return entry.displaySize
  }
  if (entry.isDirectory) {
    const s = getDisplaySize(entry.recursiveSize, entry.recursivePhysicalSize, sizeDisplayMode)
    if (s !== undefined) return sizeCellText(s, sizeFormatOpts)
    // Mirror FullList's render decision (same `getDirSizeDisplayState`): both the
    // scanning and dir states render the `<dir>` placeholder text (the scanning
    // state adds an hourglass on top, reserved separately in `sizeIconSuffixForEntry`).
    return '<dir>'
  }
  const s = getDisplaySize(entry.size, entry.physicalSize, sizeDisplayMode)
  return s !== undefined ? sizeCellText(s, sizeFormatOpts) : ''
}

/**
 * Running maxima for the date column. `total` is the row width when the date
 * is not split; `splitLeft` and `splitRight` accumulate when at least one row
 * has a `|` split. Pulled into its own helper to keep
 * `computeFullListColumnWidths` under the lint complexity cap.
 */
interface DateMaxima {
  total: number
  splitLeft: number
  splitRight: number
}

function foldDate(current: DateMaxima, formatted: FormattedDate, measure: (text: string) => number): DateMaxima {
  const { left, right } = formatted.parts
  if (right === null) {
    const w = measure(joinSegments(left))
    return w > current.total ? { ...current, total: w } : current
  }
  const lw = measure(joinSegments(left))
  const rw = measure(joinSegments(right))
  return {
    total: current.total,
    splitLeft: lw > current.splitLeft ? lw : current.splitLeft,
    splitRight: rw > current.splitRight ? rw : current.splitRight,
  }
}

/** Pixel width of the size-column icons that follow the text for this row. */
function sizeIconSuffixForEntry(
  entry: FileEntry,
  sizeDisplayMode: 'smart' | 'logical' | 'physical',
  indexing: boolean,
  showSizeMismatchWarning: boolean,
): number {
  let suffix = 0
  if (entry.isDirectory) {
    // FullList draws the hourglass for both the `size-stale` state (a settled
    // size that may still change) and the `scanning` state (`<dir>` placeholder
    // with no size yet). Both reserve the icon width here so the shrink-wrapped
    // column doesn't clip the glyph. Mirror the same `getDirSizeDisplayState`
    // decision the renderer uses.
    const s = getDisplaySize(entry.recursiveSize, entry.recursivePhysicalSize, sizeDisplayMode)
    const state = getDirSizeDisplayState(s, indexing, entry.recursiveSizePending)
    if (state === 'size-stale' || state === 'scanning') suffix += SIZE_ICON_WIDTH
  }
  if (showSizeMismatchWarning) {
    const logical = entry.isDirectory ? entry.recursiveSize : entry.size
    const physical = entry.isDirectory ? entry.recursivePhysicalSize : entry.physicalSize
    if (hasSizeMismatch(logical, physical)) suffix += SIZE_ICON_WIDTH
  }
  return suffix
}

/**
 * Compute the shrink-wrapped Ext / Size / Modified column widths for the FullList.
 * Measures only `entries` plus the optional `parentDirStats` (shown on the ".." row).
 * Fully client-side, no disk/IPC calls.
 */
export function computeFullListColumnWidths(args: {
  entries: FileEntry[]
  parentDirStats?: DirStats | null
  formattedDate: (timestamp: number) => FormattedDate
  sizeDisplayMode: 'smart' | 'logical' | 'physical'
  indexing: boolean
  showSizeMismatchWarning: boolean
  sortBy: SortColumn
  sizeFormatOpts: SizeFormatOpts
  /** Returns `true` for paths in the TCC-restricted set so the size cell
   * widths account for the `<no perms>` override. Defaults to never-restricted. */
  isRestricted?: (path: string) => boolean
}): ColumnWidths {
  const {
    entries,
    parentDirStats,
    formattedDate,
    sizeDisplayMode,
    indexing,
    showSizeMismatchWarning,
    sortBy,
    sizeFormatOpts,
    isRestricted,
  } = args

  const measure = getMeasure()
  if (!measure) {
    return {
      ext: MIN_EXT_WIDTH,
      size: MIN_SIZE_WIDTH,
      date: MIN_DATE_WIDTH,
      dateLeft: 0,
    }
  }

  const chromeFor = (column: SortColumn): number => (sortBy === column ? HEADER_CHROME_ACTIVE : HEADER_CHROME_INACTIVE)

  // Start with header widths (the column must fit its header regardless of data).
  let extMax = measure('Ext') + chromeFor('extension')
  let sizeMax = measure('Size') + chromeFor('size')
  let dateMax = measure('Modified') + chromeFor('modified')

  // Cap on per-row Ext text width so a single pathological extension can't
  // push the rest of the row off-screen. Compared against the row's text-only
  // measurement (no chrome), so the header bound (`measure('Ext') + chrome`)
  // can still win when no real extension is wider.
  const extCap = measure(EXT_CAP_SAMPLE)

  // Per-row icons in the size column live to the right of the text; count the
  // widest icon suffix we've seen so we can add it to the data width.
  let sizeIconSuffixMax = 0

  // Track the two halves of split date cells separately so the renderer can
  // line up the right halves across rows. `splitLeft`/`splitRight` stay at 0
  // unless at least one row has a `|` split.
  let date: DateMaxima = { total: dateMax, splitLeft: 0, splitRight: 0 }

  const rowResult = foldEntries(entries, {
    measure,
    extMax,
    extCap,
    sizeMax,
    sizeIconSuffixMax,
    date,
    sizeDisplayMode,
    indexing,
    showSizeMismatchWarning,
    sizeFormatOpts,
    isRestricted,
    formattedDate,
  })
  extMax = rowResult.extMax
  sizeMax = rowResult.sizeMax
  sizeIconSuffixMax = rowResult.sizeIconSuffixMax
  date = rowResult.date
  dateMax = date.total

  // The ".." row borrows the current folder's recursive size, often the largest
  // number in the listing, so fold it in or the column snaps wider the moment it loads.
  if (parentDirStats) {
    const s = getDisplaySize(parentDirStats.recursiveSize, parentDirStats.recursivePhysicalSize, sizeDisplayMode)
    if (s !== undefined) {
      const w = measure(sizeCellText(s, sizeFormatOpts)) + sizeIconSuffixMax
      if (w > sizeMax) sizeMax = w
    }
  }

  const dateOut = finalizeDate(date, dateMax)

  return {
    ext: Math.max(MIN_EXT_WIDTH, Math.ceil(extMax + MEASUREMENT_SAFETY_PAD)),
    size: Math.max(MIN_SIZE_WIDTH, Math.ceil(sizeMax + MEASUREMENT_SAFETY_PAD)),
    date: dateOut.date,
    dateLeft: dateOut.dateLeft,
  }
}

/**
 * Per-entry fold: walks the visible entries and accumulates the max ext width,
 * max size-cell width (text + icon suffix), and the date-cell maxima. Extracted
 * from `computeFullListColumnWidths` to keep the latter under the cyclomatic
 * complexity cap; the math itself is unchanged.
 */
interface FoldEntriesContext {
  measure: (text: string) => number
  extMax: number
  extCap: number
  sizeMax: number
  sizeIconSuffixMax: number
  date: DateMaxima
  sizeDisplayMode: 'smart' | 'logical' | 'physical'
  indexing: boolean
  showSizeMismatchWarning: boolean
  sizeFormatOpts: SizeFormatOpts
  isRestricted?: (path: string) => boolean
  formattedDate: (timestamp: number) => FormattedDate
}

function foldEntries(
  entries: FileEntry[],
  ctx: FoldEntriesContext,
): { extMax: number; sizeMax: number; sizeIconSuffixMax: number; date: DateMaxima } {
  let { extMax, sizeMax, sizeIconSuffixMax, date } = ctx
  for (const entry of entries) {
    const ext = getDisplayExtension(entry.name, entry.isDirectory)
    if (ext) {
      const w = Math.min(ctx.measure(ext), ctx.extCap)
      if (w > extMax) extMax = w
    }

    const sizeText = sizeTextForEntry(
      entry,
      ctx.sizeDisplayMode,
      ctx.sizeFormatOpts,
      ctx.isRestricted?.(entry.path) ?? false,
    )
    const iconSuffix = sizeIconSuffixForEntry(entry, ctx.sizeDisplayMode, ctx.indexing, ctx.showSizeMismatchWarning)
    const rowSize = (sizeText ? ctx.measure(sizeText) : 0) + iconSuffix
    if (rowSize > sizeMax) sizeMax = rowSize
    if (iconSuffix > sizeIconSuffixMax) sizeIconSuffixMax = iconSuffix

    // `!= null` (not `!== undefined`): IPC payloads serialize `Option::None` as
    // explicit `null`, and `formattedDate(null)` returns an empty record but the
    // upstream call sites used to throw on this case; keep the guard to match
    // the historical safety net (this was the F8-after-volume-switch killer).
    if (entry.modifiedAt != null) {
      date = foldDate(date, ctx.formattedDate(entry.modifiedAt), ctx.measure)
    }
  }
  return { extMax, sizeMax, sizeIconSuffixMax, date }
}

/**
 * Picks the final `date` track width and the inline-block `.date-left` width.
 *
 * The left half gets `MEASUREMENT_SAFETY_PAD` baked into the splitTotal so the
 * inline-block `.date-left` has room to absorb canvas-vs-DOM drift before its
 * own `text-overflow: ellipsis` triggers. The right half's pad falls out of
 * the final `+ MEASUREMENT_SAFETY_PAD` on `date` (so the natural width of
 * `.date-right` doesn't overflow `.col-date`'s `overflow: hidden`).
 *
 * `dateLeft` stays at 0 when no row produced a split: that's the renderer's
 * signal to skip the split path.
 */
function finalizeDate(date: DateMaxima, dateMax: number): { date: number; dateLeft: number } {
  let total = dateMax
  if (date.splitLeft > 0 || date.splitRight > 0) {
    const splitTotal = date.splitLeft + MEASUREMENT_SAFETY_PAD + DATE_PARTS_GAP + date.splitRight
    if (splitTotal > total) total = splitTotal
  }
  return {
    date: Math.max(MIN_DATE_WIDTH, Math.ceil(total + MEASUREMENT_SAFETY_PAD)),
    dateLeft: date.splitLeft > 0 ? Math.ceil(date.splitLeft + MEASUREMENT_SAFETY_PAD) : 0,
  }
}
