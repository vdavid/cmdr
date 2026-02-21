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
