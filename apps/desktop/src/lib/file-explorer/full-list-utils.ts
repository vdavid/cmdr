/**
 * Utility functions for FullList component.
 * Extracted for testability.
 */

/** Layout constants for Full mode */
export const FULL_LIST_ROW_HEIGHT = 20
export const FULL_LIST_BUFFER_SIZE = 20

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
