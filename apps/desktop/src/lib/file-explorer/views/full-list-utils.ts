/**
 * Utility functions for FullList component.
 * Extracted for testability.
 */

import { getSetting } from '$lib/settings/settings-store'
import { getEffectiveScale, onDebouncedScaleChange } from '$lib/text-size.svelte'
import type { FileEntry } from '../types'
import { colorizeSizeString, formatSizeTriads } from '../selection/selection-info-utils'

/** Layout constants for Full mode */
export const FULL_LIST_ROW_HEIGHT = 20

/** Gets the virtualization buffer size from settings (rows above/below visible area) */
export function getVirtualizationBufferRows(): number {
  return getSetting('advanced.virtualizationBufferRows')
}

/** Calculates the number of visible items based on container height */
export function getVisibleItemsCount(containerHeight: number, rowHeight: number = FULL_LIST_ROW_HEIGHT): number {
  return Math.ceil(containerHeight / rowHeight)
}

// ============================================================================
// Name/Extension Split
// ============================================================================

/**
 * Extracts the display extension from a filename (no dot). Matches Rust sorting logic:
 * dotfiles without a secondary dot → empty, no extension → empty, otherwise last segment.
 */
export function getDisplayExtension(name: string, isDirectory: boolean): string {
  if (isDirectory) return ''
  if (name.startsWith('.') && !name.slice(1).includes('.')) return ''
  const dotPos = name.lastIndexOf('.')
  if (dotPos <= 0 || dotPos === name.length - 1) return ''
  return name.slice(dotPos + 1)
}

/**
 * Returns the filename with the display extension (and its separating dot) stripped,
 * so the name and extension columns don't duplicate the extension.
 */
export function getDisplayName(name: string, isDirectory: boolean): string {
  const ext = getDisplayExtension(name, isDirectory)
  return ext ? name.slice(0, -(ext.length + 1)) : name
}

// ============================================================================
// Date Column Width Measurement
// ============================================================================

/**
 * Base font size of the date column at scale 1, in CSS pixels. Mirrors
 * `--font-size-sm` and is multiplied by the current effective text scale
 * before being handed to canvas text measurement.
 */
const DATE_COLUMN_BASE_FONT_PX = 12

/** Extra padding for the date column width (accounts for rounding and breathing room) at scale 1 */
const DATE_COLUMN_PADDING = 8

/** Minimum date column width at scale 1 — scaled at use site so it stays proportional. */
const DATE_COLUMN_MIN_WIDTH = 70

/** Cached canvas context for text measurement (reused for performance) */
let measureCanvas: CanvasRenderingContext2D | null = null
let measureCanvasScale = 0

/**
 * Reset the cached measurement canvas when the scale settles to a new value.
 * The next `getMeasureContext` call will rebuild the context with the new
 * font size. We deliberately wait for the debounced "settled" event rather
 * than re-running on every step of a slider drag — column widths update only
 * at rest, while the CSS layer reflows live.
 */
if (typeof window !== 'undefined') {
  onDebouncedScaleChange(() => {
    measureCanvas = null
    measureCanvasScale = 0
  })
}

/**
 * Get or create a canvas context for text measurement.
 * The canvas is created once per scale and reused for performance.
 */
function getMeasureContext(): CanvasRenderingContext2D | null {
  const scale = getEffectiveScale()
  if (measureCanvas && scale === measureCanvasScale) {
    return measureCanvas
  }
  if (typeof document === 'undefined') return null
  const canvas = document.createElement('canvas')
  measureCanvas = canvas.getContext('2d')
  if (measureCanvas) {
    const px = Math.max(1, Math.round(DATE_COLUMN_BASE_FONT_PX * scale))
    measureCanvas.font = `${String(px)}px -apple-system, BlinkMacSystemFont, system-ui, sans-serif`
    measureCanvasScale = scale
  }
  return measureCanvas
}

/**
 * Measure the pixel width of text using the date column's actual font styling.
 * Uses the Canvas API for fast, accurate measurement without DOM manipulation.
 *
 * @param text Text to measure
 * @returns Width in pixels, or 0 if measurement fails
 */
function measureTextWidth(text: string): number {
  const ctx = getMeasureContext()
  if (!ctx) return 0
  return ctx.measureText(text).width
}

/**
 * Generate sample date strings for width measurement based on the format.
 * Tests various digit combinations since different digits have different widths.
 * For example, '8' is typically the widest digit in most fonts.
 *
 * @param formatFn Function that formats a timestamp to a date string
 * @returns Array of sample date strings to measure
 */
function generateSampleDateStrings(formatFn: (timestamp: number) => string): string[] {
  // Create dates with various digit patterns to find the maximum width.
  // We use real dates that produce the desired digit patterns when formatted.
  // Note: Month is 0-indexed in Date constructor.
  const sampleDates = [
    // Dates that produce various digit patterns
    new Date(1111, 10, 11, 11, 11, 11), // "1111-11-11 11:11" pattern
    new Date(2022, 11, 22, 22, 22, 22), // "2022-12-22 22:22" pattern
    new Date(2028, 7, 28, 18, 28, 28), // Contains many 8s (typically widest)
    new Date(2000, 9, 10, 10, 10, 10), // "2000-10-10 10:10" pattern with 0s
    new Date(2024, 11, 31, 23, 59, 59), // End of year/day (large numbers)
    new Date(2024, 0, 1, 0, 0, 0), // Start of year/day
    new Date(2088, 7, 8, 8, 8, 8), // Many 8s for maximum width
    new Date(2008, 8, 8, 8, 8, 8), // Another 8-heavy date
  ]

  // Convert dates to Unix timestamps (seconds) and format
  return sampleDates.map((date) => formatFn(date.getTime() / 1000))
}

/**
 * Measure the optimal date column width based on the current date/time format.
 * Tests multiple sample strings to find the maximum width needed to display
 * any possible date without truncation.
 *
 * @param formatFn Function that formats a timestamp (seconds) to a date string
 * @returns Optimal column width in pixels
 */
export function measureDateColumnWidth(formatFn: (timestamp: number) => string): number {
  const samples = generateSampleDateStrings(formatFn)

  // Measure each sample and find the maximum width
  let maxWidth = 0
  for (const sample of samples) {
    const width = measureTextWidth(sample)
    if (width > maxWidth) {
      maxWidth = width
    }
  }

  // Add padding and enforce minimum width. Both scale with text size so the
  // column stays proportional at large scales — at 200%, an 8 px breathing
  // gap looks pinched.
  // Use Math.ceil to avoid subpixel rendering issues.
  const scale = getEffectiveScale()
  const minWidth = Math.round(DATE_COLUMN_MIN_WIDTH * scale)
  const padding = Math.round(DATE_COLUMN_PADDING * scale)
  return Math.max(minWidth, Math.ceil(maxWidth) + padding)
}

// ============================================================================
// Display-size override (virtual git entries)
// ============================================================================

/**
 * What the Size column should render for one row. Virtual git entries
 * carry a `displaySize` string that's rendered verbatim (`+12 / -3`,
 * `5 files`, `on main`, …); regular rows render formatted bytes from
 * `size` / `physicalSize`. This helper centralizes the decision so the
 * width-measurer and the renderer agree on what's drawn.
 */
export interface SizeDisplayPick {
  /** When set, render this string verbatim. */
  override?: string
  /** Optional rich tooltip for the override (also used as aria-label). */
  tooltip?: string
}

/**
 * Picks the Size-column override for `entry`. Returns `{}` for normal
 * rows (the renderer falls through to byte formatting).
 *
 * When `isRestricted` is true, the entry is in the runtime TCC-restricted
 * set (see `$lib/stores/restricted-paths-store`). We override the size
 * column with `<no perms>` because the indexer recorded `recursiveSize=0`
 * for the folder (couldn't read its contents), and rendering literal `0`
 * misleads the user into thinking the folder is empty. The override takes
 * priority over `entry.displaySize` since restricted state is the more
 * actionable signal.
 */
export function pickSizeDisplay(entry: FileEntry, isRestricted = false): SizeDisplayPick {
  if (isRestricted) {
    return { override: '<no perms>', tooltip: 'Cmdr lacks permission to read this folder' }
  }
  if (entry.displaySize != null) {
    return { override: entry.displaySize, tooltip: entry.displaySizeTooltip ?? undefined }
  }
  return {}
}

// ============================================================================
// Size Display Mode Helpers
// ============================================================================

/**
 * Picks the display size based on the user's size display preference.
 *
 * Accepts both `null` and `undefined` for missing values. The wire format from
 * the Rust backend serializes Optional fields as `null` after the typed-IPC
 * migration (Group A: `skip_serializing_if` was removed so specta accepts the
 * types in Unified mode), but legacy callers and tests still pass `undefined`.
 * Internal checks use `!= null` to handle both cleanly.
 */
export function getDisplaySize(
  logical: number | null | undefined,
  physical: number | null | undefined,
  mode: 'smart' | 'logical' | 'physical',
): number | undefined {
  // Coerce null → undefined at the boundary so the return type matches what
  // downstream consumers (`!= null` in render guards) expect.
  if (mode === 'logical') return logical ?? undefined
  // Fall back to logical when physical is unavailable — a visible size is better than blank.
  if (mode === 'physical') return physical ?? logical ?? undefined
  // smart: min of available values, but show logical for cloud/dataless files (physical=0, logical>0)
  if (logical != null && physical != null) return physical > 0 ? Math.min(logical, physical) : logical
  return logical ?? physical ?? undefined
}

/**
 * Whether content and on-disk sizes differ enough to warrant a warning icon.
 * Both conditions must be true: ≥50% relative difference AND ≥200 MB absolute difference.
 */
// Group A wire-format: callers pass IPC-derived `number | null` values directly.
// Use `== null` so both `null` and `undefined` count as "no value".
export function hasSizeMismatch(logical: number | null | undefined, physical: number | null | undefined): boolean {
  if (logical == null || physical == null) return false
  if (logical === 0 || physical === 0) return false
  const diff = Math.abs(logical - physical)
  const smaller = Math.min(logical, physical)
  return diff >= smaller * 0.5 && diff >= 200_000_000
}

/** Formats a byte count as colored HTML digit triads (same colors as the size column). */
function formatBytesHtml(bytes: number): string {
  return formatSizeTriads(bytes)
    .map((t) => `<span class="${t.tierClass}">${t.value}</span>`)
    .join('')
}

/** Formats a single size line: "Label: 1.23 GB (1 234 567 890 bytes)" with colored triads and a colored unit-tagged value. */
function sizeLineHtml(label: string, bytes: number, formatSize: (b: number) => string): string {
  return `${label}: ${colorizeSizeString(formatSize(bytes))} (${formatBytesHtml(bytes)} bytes)`
}

/**
 * Build a rich HTML tooltip for a file showing both content and on-disk sizes.
 * Always shows both lines when both sizes are available. Falls back to a single line otherwise.
 */
export function buildFileSizeTooltip(
  logical: number | null | undefined,
  physical: number | null | undefined,
  formatSize: (bytes: number) => string,
): string | { html: string } {
  // Group A wire-format: IPC sends `null`, not `undefined`. Use `!= null` to handle both.
  if (logical == null && physical == null) return ''
  if (logical != null && physical != null) {
    return {
      html: `${sizeLineHtml('Content', logical, formatSize)}<br>${sizeLineHtml('On disk', physical, formatSize)}`,
    }
  }
  const size = logical ?? physical
  if (size == null) return ''
  return { html: `${colorizeSizeString(formatSize(size))} (${formatBytesHtml(size)} bytes)` }
}

/**
 * Build a rich HTML tooltip for the selection summary bar.
 * Shows "Selected" and "Of total" lines, with a separate "On disk" section when physical sizes are available.
 */
export function buildSelectionSizeTooltip(
  selectedLogical: number,
  selectedPhysical: number,
  totalLogical: number,
  totalPhysical: number,
  formatSize: (bytes: number) => string,
): { html: string } | undefined {
  if (totalLogical <= 0) return undefined

  const selLine = (label: string, bytes: number) =>
    `${label}: ${colorizeSizeString(formatSize(bytes))} (${formatBytesHtml(bytes)} bytes)`
  const lines: string[] = [selLine('Selected', selectedLogical), selLine('Of total', totalLogical)]

  if (totalPhysical > 0) {
    lines.push('', 'On disk:', selLine('Selected', selectedPhysical), selLine('Of total', totalPhysical))
  }

  return { html: lines.join('<br>') }
}

// ============================================================================
// Directory Size Display Helpers
// ============================================================================

/** Display state for a directory's size column in FullList. */
export type DirSizeDisplayState = 'dir' | 'scanning' | 'size' | 'size-stale'

/**
 * Determine the display state for a directory's size column.
 *
 * Rules:
 * - Has recursiveSize + indexing active -> 'size-stale' (show size with stale warning)
 * - Has recursiveSize + not indexing    -> 'size' (show formatted size)
 * - No recursiveSize + indexing active  -> 'scanning' (show spinner)
 * - No recursiveSize + not indexing     -> 'dir' (show <dir> placeholder)
 *
 * "Indexing active" means scanning OR aggregating — sizes aren't ready until aggregation finishes.
 */
export function getDirSizeDisplayState(
  recursiveSize: number | null | undefined,
  indexing: boolean,
): DirSizeDisplayState {
  // `!= null` covers both `null` (post-Group-A wire format) and `undefined`
  // (legacy/tests). See `getDisplaySize` for the migration context.
  if (recursiveSize != null) {
    return indexing ? 'size-stale' : 'size'
  }
  return indexing ? 'scanning' : 'dir'
}

/**
 * Build the tooltip string for a directory's size column.
 *
 * @param recursiveSize - The recursive size in bytes, or undefined if not yet computed.
 * @param recursivePhysicalSize - The recursive physical size in bytes, or undefined.
 * @param recursiveFileCount - The recursive file count, or 0 if unknown.
 * @param recursiveDirCount - The recursive folder count, or 0 if unknown.
 * @param scanning - Whether a scan is currently active.
 * @param formatSize - Function to format bytes as a human-readable string.
 * @param formatNum - Function to format a number with locale separators.
 * @param plural - Function to pick singular/plural form.
 */
export function buildDirSizeTooltip(
  recursiveSize: number | null | undefined,
  recursivePhysicalSize: number | null | undefined,
  recursiveFileCount: number,
  recursiveDirCount: number,
  scanning: boolean,
  formatSize: (bytes: number) => string,
  formatNum: (n: number) => string,
  plural: (count: number, singular: string, pluralForm: string) => string,
): string | { html: string } {
  if (recursiveSize != null) {
    const lines: string[] = []

    // Size lines with colored byte triads
    if (recursivePhysicalSize != null) {
      lines.push(sizeLineHtml('Content', recursiveSize, formatSize))
      lines.push(sizeLineHtml('On disk', recursivePhysicalSize, formatSize))
    } else {
      lines.push(`${colorizeSizeString(formatSize(recursiveSize))} (${formatBytesHtml(recursiveSize)} bytes)`)
    }

    // File/folder counts with "no" for zero
    const filesStr =
      recursiveFileCount === 0
        ? 'No files'
        : `${formatNum(recursiveFileCount)} ${plural(recursiveFileCount, 'file', 'files')}`
    const foldersStr =
      recursiveDirCount === 0
        ? 'no folders'
        : `${formatNum(recursiveDirCount)} ${plural(recursiveDirCount, 'folder', 'folders')}`
    lines.push(`${filesStr}, ${foldersStr}`)

    if (scanning) {
      lines.push('Updating index \u2014 size may change.')
    }

    return { html: lines.join('<br>') }
  }
  return scanning ? 'Scanning...' : ''
}
