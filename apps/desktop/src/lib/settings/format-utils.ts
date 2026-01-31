/**
 * Formatting utilities for settings-based display.
 * These functions are pure and don't need reactive state.
 */

import type { DateTimeFormat } from './types'

/**
 * Format a timestamp according to the given format.
 * @param timestamp Unix timestamp in seconds
 * @param format The date/time format to use
 * @param customFormat Custom format string (used when format is 'custom')
 */
export function formatDateTimeWithFormat(
    timestamp: number | undefined,
    format: DateTimeFormat,
    customFormat: string,
): string {
    if (timestamp === undefined) return ''

    const date = new Date(timestamp * 1000)

    switch (format) {
        case 'system':
            return date.toLocaleString()

        case 'iso':
            return formatIso(date)

        case 'short':
            return formatShort(date)

        case 'custom':
            return formatCustom(date, customFormat)

        default:
            return formatIso(date)
    }
}

function formatIso(date: Date): string {
    const pad = (n: number) => String(n).padStart(2, '0')
    const year = date.getFullYear()
    const month = pad(date.getMonth() + 1)
    const day = pad(date.getDate())
    const hours = pad(date.getHours())
    const mins = pad(date.getMinutes())
    return `${String(year)}-${month}-${day} ${hours}:${mins}`
}

function formatShort(date: Date): string {
    const pad = (n: number) => String(n).padStart(2, '0')
    const month = pad(date.getMonth() + 1)
    const day = pad(date.getDate())
    const hours = pad(date.getHours())
    const mins = pad(date.getMinutes())
    return `${month}/${day} ${hours}:${mins}`
}

function formatCustom(date: Date, format: string): string {
    const pad = (n: number) => String(n).padStart(2, '0')
    return format
        .replace('YYYY', String(date.getFullYear()))
        .replace('MM', pad(date.getMonth() + 1))
        .replace('DD', pad(date.getDate()))
        .replace('HH', pad(date.getHours()))
        .replace('mm', pad(date.getMinutes()))
        .replace('ss', pad(date.getSeconds()))
}
