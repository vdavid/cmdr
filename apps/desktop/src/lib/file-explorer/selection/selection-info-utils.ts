/**
 * Utility functions for SelectionInfo component.
 * Extracted for testability.
 */

import type { FileEntry } from '../types'

// Size tier colors for digit triads (indexed: 0=bytes, 1=kB, 2=MB, 3=GB, 4=TB+)
export const sizeTierClasses = ['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']

/** Formats a number into digit triads with separate styled spans */
export function formatSizeTriads(bytes: number): { value: string; tierClass: string }[] {
    const str = String(bytes)
    const triads: { value: string; tierClass: string }[] = []

    // Split into triads from right to left
    let remaining = str
    let tierIndex = 0
    while (remaining.length > 0) {
        const start = Math.max(0, remaining.length - 3)
        const triad = remaining.slice(start)
        remaining = remaining.slice(0, start)

        triads.unshift({
            value: triad,
            tierClass: sizeTierClasses[Math.min(tierIndex, sizeTierClasses.length - 1)],
        })
        tierIndex++
    }

    // Add thousand separators between triads (space)
    return triads.map((t, i) => ({
        ...t,
        value: i < triads.length - 1 ? t.value + '\u2009' : t.value, // thin space separator
    }))
}

/** Formats bytes as human-readable (for tooltip) */
export function formatHumanReadable(bytes: number): string {
    const units = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB']
    let value = bytes
    let unitIndex = 0
    while (value >= 1024 && unitIndex < units.length - 1) {
        value /= 1024
        unitIndex++
    }
    const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
    return `${valueStr} ${units[unitIndex]}`
}

/** Formats timestamp as YYYY-MM-DD hh:mm:ss */
export function formatDate(timestamp: number | undefined): string {
    if (timestamp === undefined) return ''
    const date = new Date(timestamp * 1000)
    const pad = (n: number) => String(n).padStart(2, '0')
    const year = date.getFullYear()
    const month = pad(date.getMonth() + 1)
    const day = pad(date.getDate())
    const hours = pad(date.getHours())
    const mins = pad(date.getMinutes())
    const secs = pad(date.getSeconds())
    return `${String(year)}-${month}-${day} ${hours}:${mins}:${secs}`
}

/** Builds date tooltip content */
export function buildDateTooltip(e: FileEntry): string {
    const lines: string[] = []
    if (e.createdAt !== undefined) lines.push(`Created: ${formatDate(e.createdAt)}`)
    if (e.openedAt !== undefined) lines.push(`Last opened: ${formatDate(e.openedAt)}`)
    if (e.addedAt !== undefined) lines.push(`Last moved ("added"): ${formatDate(e.addedAt)}`)
    if (e.modifiedAt !== undefined) lines.push(`Last modified: ${formatDate(e.modifiedAt)}`)
    return lines.join('\n')
}

/** Determines size display for an entry */
export function getSizeDisplay(
    entry: FileEntry | null,
    isBrokenSymlink: boolean,
    isPermissionDenied: boolean,
): { value: string; tierClass: string }[] | 'DIR' | null {
    if (!entry || isBrokenSymlink || isPermissionDenied) return null
    if (entry.isDirectory) return 'DIR'
    if (entry.size === undefined) return null
    return formatSizeTriads(entry.size)
}

/** Determines date display for an entry */
export function getDateDisplay(
    entry: FileEntry | null,
    isBrokenSymlink: boolean,
    isPermissionDenied: boolean,
    currentDirModifiedAt?: number,
): string {
    if (!entry) return ''
    if (isBrokenSymlink) return '(broken symlink)'
    if (isPermissionDenied) return '(permission denied)'
    // For ".." entry, use the current directory's modified time
    const timestamp = entry.name === '..' ? currentDirModifiedAt : entry.modifiedAt
    return formatDate(timestamp)
}

/** Checks if entry is a broken symlink */
export function isBrokenSymlink(entry: FileEntry | null): boolean {
    return entry !== null && entry.isSymlink && entry.iconId === 'symlink-broken'
}

/** Checks if entry has permission denied */
export function isPermissionDenied(entry: FileEntry | null): boolean {
    return entry !== null && !entry.isSymlink && entry.permissions === 0 && entry.size === undefined
}

// ============================================================================
// Selection summary utilities
// ============================================================================

/** Formats a count with proper singular/plural form */
export function pluralize(count: number, singular: string, plural: string): string {
    return count === 1 ? singular : plural
}

/** Formats a number with thousands separators using en-US locale */
export function formatNumber(n: number): string {
    return n.toLocaleString('en-US')
}

/** Calculates percentage, rounded to nearest integer */
export function calculatePercentage(part: number, total: number): number {
    if (total === 0) return 0
    return Math.round((part / total) * 100)
}
