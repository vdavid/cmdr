/**
 * Utility functions for FullList component.
 * Extracted for testability.
 */

import { getSetting } from '$lib/settings/settings-store'

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

/** Formats timestamp as YYYY-MM-DD hh:mm (shorter than SelectionInfo format) */
export function formatDateShort(timestamp: number | undefined): string {
    if (timestamp === undefined) return ''
    const date = new Date(timestamp * 1000)
    const pad = (n: number) => String(n).padStart(2, '0')
    const year = date.getFullYear()
    const month = pad(date.getMonth() + 1)
    const day = pad(date.getDate())
    const hours = pad(date.getHours())
    const mins = pad(date.getMinutes())
    return `${String(year)}-${month}-${day} ${hours}:${mins}`
}

// ============================================================================
// Date Column Width Measurement
// ============================================================================

/**
 * The date column font specification matching CSS: var(--font-size-sm) = 12px, system font.
 * Used for accurate text width measurement via Canvas API.
 */
const DATE_COLUMN_FONT = '12px -apple-system, BlinkMacSystemFont, system-ui, sans-serif'

/** Extra padding for the date column width (accounts for rounding and breathing room) */
const DATE_COLUMN_PADDING = 8

/** Minimum date column width to prevent collapsing on very short formats */
const DATE_COLUMN_MIN_WIDTH = 70

/** Cached canvas context for text measurement (reused for performance) */
let measureCanvas: CanvasRenderingContext2D | null = null

/**
 * Get or create a canvas context for text measurement.
 * The canvas is created once and reused for performance.
 */
function getMeasureContext(): CanvasRenderingContext2D | null {
    if (!measureCanvas) {
        // Check if we're in a browser environment (may be SSR)
        if (typeof document === 'undefined') return null
        const canvas = document.createElement('canvas')
        measureCanvas = canvas.getContext('2d')
        if (measureCanvas) {
            measureCanvas.font = DATE_COLUMN_FONT
        }
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

    // Add padding and enforce minimum width
    // Use Math.ceil to avoid subpixel rendering issues
    return Math.max(DATE_COLUMN_MIN_WIDTH, Math.ceil(maxWidth) + DATE_COLUMN_PADDING)
}

// ============================================================================
// Size Display Mode Helpers
// ============================================================================

/** Picks the display size based on the user's size display preference. */
export function getDisplaySize(
    logical: number | undefined,
    physical: number | undefined,
    mode: 'smart' | 'logical' | 'physical',
): number | undefined {
    if (mode === 'logical') return logical
    // Fall back to logical when physical is unavailable — a visible size is better than blank.
    if (mode === 'physical') return physical ?? logical
    // smart: min of available values
    if (logical !== undefined && physical !== undefined) return Math.min(logical, physical)
    return logical ?? physical
}

/** Whether two sizes differ enough to show both (>1% difference). */
export function sizesDifferSignificantly(a: number, b: number): boolean {
    if (a === 0 && b === 0) return false
    const larger = Math.max(a, b)
    return Math.abs(a - b) / larger > 0.01
}

/**
 * Build a tooltip for a file showing both content and on-disk sizes when they differ.
 * Returns the single formatted size when they're essentially equal or only one is available.
 */
export function buildFileSizeTooltip(
    logical: number | undefined,
    physical: number | undefined,
    formatSize: (bytes: number) => string,
): string {
    if (logical === undefined && physical === undefined) return ''
    if (logical !== undefined && physical !== undefined && sizesDifferSignificantly(logical, physical)) {
        return `Content: ${formatSize(logical)} \u00B7 On disk: ${formatSize(physical)}`
    }
    // Show whichever is available (or both are equal — just show one)
    const size = logical ?? physical
    return size !== undefined ? formatSize(size) : ''
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
export function getDirSizeDisplayState(recursiveSize: number | undefined, indexing: boolean): DirSizeDisplayState {
    if (recursiveSize !== undefined) {
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
    recursiveSize: number | undefined,
    recursivePhysicalSize: number | undefined,
    recursiveFileCount: number,
    recursiveDirCount: number,
    scanning: boolean,
    formatSize: (bytes: number) => string,
    formatNum: (n: number) => string,
    plural: (count: number, singular: string, pluralForm: string) => string,
): string {
    if (recursiveSize !== undefined) {
        const filesStr = `${formatNum(recursiveFileCount)} ${plural(recursiveFileCount, 'file', 'files')}`
        const foldersStr = `${formatNum(recursiveDirCount)} ${plural(recursiveDirCount, 'folder', 'folders')}`

        // Show both sizes when they differ significantly
        let sizeStr: string
        if (recursivePhysicalSize !== undefined && sizesDifferSignificantly(recursiveSize, recursivePhysicalSize)) {
            sizeStr = `Content: ${formatSize(recursiveSize)} \u00B7 On disk: ${formatSize(recursivePhysicalSize)}`
        } else {
            sizeStr = formatSize(recursiveSize)
        }

        const base = `${sizeStr} \u00B7 ${filesStr} \u00B7 ${foldersStr}`
        return scanning ? `${base} \u2014 Might be outdated. Currently scanning...` : base
    }
    return scanning ? 'Scanning...' : ''
}
