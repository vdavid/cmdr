/**
 * Formatting utilities for settings-based display.
 * These functions are pure and don't need reactive state.
 */

import type { DateTimeFormat, FileSizeFormat } from './types'

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

// ============================================================================
// File Size Formatting
// ============================================================================

// Binary units (base 1024) - traditional computing units
const binaryUnits = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB']

// SI units (base 1000) - International System of Units
const siUnits = ['bytes', 'kB', 'MB', 'GB', 'TB', 'PB']

/**
 * Format bytes as human-readable string based on the format setting.
 * @param bytes Number of bytes
 * @param format 'binary' uses 1024-based (KB/MB/GB), 'si' uses 1000-based (kB/MB/GB)
 */
export function formatFileSizeWithFormat(bytes: number, format: FileSizeFormat): string {
    const base = format === 'binary' ? 1024 : 1000
    const units = format === 'binary' ? binaryUnits : siUnits

    let value = bytes
    let unitIndex = 0
    while (value >= base && unitIndex < units.length - 1) {
        value /= base
        unitIndex++
    }

    const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
    return `${valueStr} ${units[unitIndex]}`
}
