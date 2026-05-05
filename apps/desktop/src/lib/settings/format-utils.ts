/**
 * Formatting utilities for settings-based display.
 * These functions are pure and don't need reactive state.
 */

import type { DateTimeFormat, FileSizeFormat } from './types'

/**
 * Two halves of a formatted date/time. The optional `|` character in a format
 * string splits each rendered cell into a left half (typically the date) and
 * a right half (typically the time), so the file list can align time digits
 * across rows even when the date width varies.
 */
export interface DateTimeParts {
  /** Always present. The portion of the formatted string before any `|`. */
  left: string
  /** The portion after the `|`, or `null` if the format has no split. */
  right: string | null
}

/**
 * Format a timestamp into its two halves according to the given format.
 * Returns `{ left: '', right: null }` for missing timestamps (`null`,
 * `undefined`, or zero — virtual git entries that have no meaningful date land
 * on `null` over the wire but `0` from a few legacy code paths; treat both as
 * absent).
 *
 * The optional `|` in a format string splits the output. Whitespace around the
 * `|` is trimmed; the file-list renderer adds its own gap between the two
 * halves. A degenerate format with an empty right side (e.g. `YYYY-MM-DD |`)
 * is treated as no split.
 *
 * @param timestamp Unix timestamp in seconds
 * @param format The date/time format to use
 * @param customFormat Custom format string (used when format is 'custom')
 */
export function formatDateTimePartsWithFormat(
  timestamp: number | null | undefined,
  format: DateTimeFormat,
  customFormat: string,
): DateTimeParts {
  if (timestamp == null || timestamp === 0) return { left: '', right: null }

  const date = new Date(timestamp * 1000)

  switch (format) {
    case 'system':
      return { left: date.toLocaleString(), right: null }

    case 'iso':
      return formatCustomParts(date, 'YYYY-MM-DD | HH:mm')

    case 'short':
      return formatCustomParts(date, 'MM/DD | HH:mm')

    case 'custom':
      return formatCustomParts(date, customFormat)

    default:
      return formatCustomParts(date, 'YYYY-MM-DD | HH:mm')
  }
}

/**
 * Format a timestamp as a single string (parts joined with a space).
 * Use this for tooltips, MCP responses, and anywhere a one-line label is
 * wanted. The file list itself uses {@link formatDateTimePartsWithFormat}
 * so it can align the right halves of the cells.
 */
export function formatDateTimeWithFormat(
  timestamp: number | null | undefined,
  format: DateTimeFormat,
  customFormat: string,
): string {
  const parts = formatDateTimePartsWithFormat(timestamp, format, customFormat)
  if (parts.right === null) return parts.left
  return `${parts.left} ${parts.right}`
}

function formatCustomParts(date: Date, format: string): DateTimeParts {
  const pipeIdx = format.indexOf('|')
  if (pipeIdx < 0) {
    return { left: applyTokens(date, format), right: null }
  }
  const leftFmt = format.slice(0, pipeIdx).trimEnd()
  const rightFmt = format.slice(pipeIdx + 1).trimStart()
  const left = applyTokens(date, leftFmt)
  const right = applyTokens(date, rightFmt)
  // Treat empty right (e.g. `YYYY-MM-DD |`) as no-split.
  if (right === '') return { left, right: null }
  return { left, right }
}

function applyTokens(date: Date, format: string): string {
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
