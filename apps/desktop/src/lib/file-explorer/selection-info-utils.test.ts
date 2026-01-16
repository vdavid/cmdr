/**
 * Tests for selection-info-utils.ts
 */
import { describe, it, expect } from 'vitest'
import {
    formatSizeTriads,
    formatHumanReadable,
    formatDate,
    buildDateTooltip,
    getSizeDisplay,
    getDateDisplay,
    isBrokenSymlink,
    isPermissionDenied,
    sizeTierClasses,
} from './selection-info-utils'
import type { FileEntry } from './types'

// Helper to create a basic file entry
function createFileEntry(overrides: Partial<FileEntry> = {}): FileEntry {
    return {
        name: 'test.txt',
        path: '/test/test.txt',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'staff',
        iconId: 'file',
        extendedMetadataLoaded: true,
        ...overrides,
    }
}

describe('formatSizeTriads', () => {
    it('formats single digit', () => {
        const result = formatSizeTriads(5)
        expect(result).toHaveLength(1)
        expect(result[0].value).toBe('5')
        expect(result[0].tierClass).toBe('size-bytes')
    })

    it('formats two digits', () => {
        const result = formatSizeTriads(42)
        expect(result).toHaveLength(1)
        expect(result[0].value).toBe('42')
        expect(result[0].tierClass).toBe('size-bytes')
    })

    it('formats three digits (no separator needed)', () => {
        const result = formatSizeTriads(999)
        expect(result).toHaveLength(1)
        expect(result[0].value).toBe('999')
        expect(result[0].tierClass).toBe('size-bytes')
    })

    it('formats four digits (KB range)', () => {
        const result = formatSizeTriads(1234)
        expect(result).toHaveLength(2)
        expect(result[0].value).toBe('1\u2009') // with thin space separator
        expect(result[0].tierClass).toBe('size-kb')
        expect(result[1].value).toBe('234')
        expect(result[1].tierClass).toBe('size-bytes')
    })

    it('formats six digits', () => {
        const result = formatSizeTriads(123456)
        expect(result).toHaveLength(2)
        expect(result[0].value).toBe('123\u2009')
        expect(result[0].tierClass).toBe('size-kb')
        expect(result[1].value).toBe('456')
        expect(result[1].tierClass).toBe('size-bytes')
    })

    it('formats seven digits (MB range)', () => {
        const result = formatSizeTriads(1234567)
        expect(result).toHaveLength(3)
        expect(result[0].value).toBe('1\u2009')
        expect(result[0].tierClass).toBe('size-mb')
        expect(result[1].value).toBe('234\u2009')
        expect(result[1].tierClass).toBe('size-kb')
        expect(result[2].value).toBe('567')
        expect(result[2].tierClass).toBe('size-bytes')
    })

    it('formats ten digits (GB range)', () => {
        const result = formatSizeTriads(1234567890)
        expect(result).toHaveLength(4)
        expect(result[0].tierClass).toBe('size-gb')
        expect(result[1].tierClass).toBe('size-mb')
        expect(result[2].tierClass).toBe('size-kb')
        expect(result[3].tierClass).toBe('size-bytes')
    })

    it('formats thirteen digits (TB range)', () => {
        const result = formatSizeTriads(1234567890123)
        expect(result).toHaveLength(5)
        expect(result[0].tierClass).toBe('size-tb')
        expect(result[1].tierClass).toBe('size-gb')
    })

    it('caps at TB tier for very large numbers', () => {
        const result = formatSizeTriads(1234567890123456)
        expect(result).toHaveLength(6)
        // Both highest tiers should be size-tb (capped)
        expect(result[0].tierClass).toBe('size-tb')
        expect(result[1].tierClass).toBe('size-tb')
    })

    it('handles zero', () => {
        const result = formatSizeTriads(0)
        expect(result).toHaveLength(1)
        expect(result[0].value).toBe('0')
        expect(result[0].tierClass).toBe('size-bytes')
    })
})

describe('formatHumanReadable', () => {
    it('formats bytes', () => {
        expect(formatHumanReadable(0)).toBe('0 bytes')
        expect(formatHumanReadable(1)).toBe('1 bytes')
        expect(formatHumanReadable(512)).toBe('512 bytes')
        expect(formatHumanReadable(1023)).toBe('1023 bytes')
    })

    it('formats kilobytes', () => {
        expect(formatHumanReadable(1024)).toBe('1.00 KB')
        expect(formatHumanReadable(1536)).toBe('1.50 KB')
        expect(formatHumanReadable(10240)).toBe('10.00 KB')
    })

    it('formats megabytes', () => {
        expect(formatHumanReadable(1048576)).toBe('1.00 MB')
        expect(formatHumanReadable(5242880)).toBe('5.00 MB')
    })

    it('formats gigabytes', () => {
        expect(formatHumanReadable(1073741824)).toBe('1.00 GB')
        expect(formatHumanReadable(10737418240)).toBe('10.00 GB')
    })

    it('formats terabytes', () => {
        expect(formatHumanReadable(1099511627776)).toBe('1.00 TB')
    })

    it('formats petabytes', () => {
        expect(formatHumanReadable(1125899906842624)).toBe('1.00 PB')
    })

    it('caps at petabytes for very large values', () => {
        // Even larger than PB stays at PB
        const result = formatHumanReadable(1125899906842624 * 1024)
        expect(result).toContain('PB')
    })
})

describe('formatDate', () => {
    it('returns empty string for undefined', () => {
        expect(formatDate(undefined)).toBe('')
    })

    it('formats Unix timestamp correctly', () => {
        // Jan 15, 2024 12:30:45 UTC
        const timestamp = 1705322445
        const result = formatDate(timestamp)
        // Result depends on local timezone, but format should be YYYY-MM-DD HH:MM:SS
        expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
    })

    it('formats epoch timestamp', () => {
        const result = formatDate(0)
        expect(result).toMatch(/^1970-01-01/)
    })

    it('pads single digit months and days', () => {
        // Timestamp for a date with single digit month/day
        const timestamp = 1609459200 // Jan 1, 2021 00:00:00 UTC
        const result = formatDate(timestamp)
        expect(result).toContain('-01-')
    })
})

describe('buildDateTooltip', () => {
    it('returns empty string when no dates are set', () => {
        const entry = createFileEntry()
        expect(buildDateTooltip(entry)).toBe('')
    })

    it('includes created date', () => {
        const entry = createFileEntry({ createdAt: 1705322445 })
        const result = buildDateTooltip(entry)
        expect(result).toContain('Created:')
    })

    it('includes opened date', () => {
        const entry = createFileEntry({ openedAt: 1705322445 })
        const result = buildDateTooltip(entry)
        expect(result).toContain('Last opened:')
    })

    it('includes added date', () => {
        const entry = createFileEntry({ addedAt: 1705322445 })
        const result = buildDateTooltip(entry)
        expect(result).toContain('Last moved ("added"):')
    })

    it('includes modified date', () => {
        const entry = createFileEntry({ modifiedAt: 1705322445 })
        const result = buildDateTooltip(entry)
        expect(result).toContain('Last modified:')
    })

    it('includes multiple dates separated by newlines', () => {
        const entry = createFileEntry({
            createdAt: 1705322445,
            modifiedAt: 1705408845,
        })
        const result = buildDateTooltip(entry)
        expect(result).toContain('Created:')
        expect(result).toContain('Last modified:')
        expect(result).toContain('\n')
    })
})

describe('getSizeDisplay', () => {
    it('returns null for null entry', () => {
        expect(getSizeDisplay(null, false, false)).toBeNull()
    })

    it('returns null for broken symlink', () => {
        const entry = createFileEntry({ size: 1000 })
        expect(getSizeDisplay(entry, true, false)).toBeNull()
    })

    it('returns null for permission denied', () => {
        const entry = createFileEntry({ size: 1000 })
        expect(getSizeDisplay(entry, false, true)).toBeNull()
    })

    it('returns DIR for directory', () => {
        const entry = createFileEntry({ isDirectory: true })
        expect(getSizeDisplay(entry, false, false)).toBe('DIR')
    })

    it('returns null for file with undefined size', () => {
        const entry = createFileEntry({ size: undefined })
        expect(getSizeDisplay(entry, false, false)).toBeNull()
    })

    it('returns formatted triads for file with size', () => {
        const entry = createFileEntry({ size: 1234567 })
        const result = getSizeDisplay(entry, false, false)
        expect(result).not.toBe('DIR')
        expect(result).not.toBeNull()
        expect(Array.isArray(result)).toBe(true)
    })
})

describe('getDateDisplay', () => {
    it('returns empty string for null entry', () => {
        expect(getDateDisplay(null, false, false)).toBe('')
    })

    it('returns broken symlink message', () => {
        const entry = createFileEntry()
        expect(getDateDisplay(entry, true, false)).toBe('(broken symlink)')
    })

    it('returns permission denied message', () => {
        const entry = createFileEntry()
        expect(getDateDisplay(entry, false, true)).toBe('(permission denied)')
    })

    it('returns formatted date for regular file', () => {
        const entry = createFileEntry({ modifiedAt: 1705322445 })
        const result = getDateDisplay(entry, false, false)
        expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
    })

    it('uses currentDirModifiedAt for parent entry', () => {
        const entry = createFileEntry({ name: '..', modifiedAt: 1000 })
        const result = getDateDisplay(entry, false, false, 2000)
        // Should use currentDirModifiedAt (2000) instead of entry.modifiedAt (1000)
        const resultWithoutOverride = getDateDisplay(entry, false, false)
        expect(result).not.toBe(resultWithoutOverride)
    })

    it('returns empty string when no modified time and not parent', () => {
        const entry = createFileEntry({ modifiedAt: undefined })
        expect(getDateDisplay(entry, false, false)).toBe('')
    })
})

describe('isBrokenSymlink', () => {
    it('returns false for null', () => {
        expect(isBrokenSymlink(null)).toBe(false)
    })

    it('returns false for regular file', () => {
        const entry = createFileEntry()
        expect(isBrokenSymlink(entry)).toBe(false)
    })

    it('returns false for valid symlink', () => {
        const entry = createFileEntry({ isSymlink: true, iconId: 'symlink' })
        expect(isBrokenSymlink(entry)).toBe(false)
    })

    it('returns true for broken symlink', () => {
        const entry = createFileEntry({ isSymlink: true, iconId: 'symlink-broken' })
        expect(isBrokenSymlink(entry)).toBe(true)
    })
})

describe('isPermissionDenied', () => {
    it('returns false for null', () => {
        expect(isPermissionDenied(null)).toBe(false)
    })

    it('returns false for regular file with permissions', () => {
        const entry = createFileEntry({ permissions: 0o644, size: 100 })
        expect(isPermissionDenied(entry)).toBe(false)
    })

    it('returns false for symlink even with no permissions', () => {
        const entry = createFileEntry({ isSymlink: true, permissions: 0, size: undefined })
        expect(isPermissionDenied(entry)).toBe(false)
    })

    it('returns true for non-symlink with no permissions and no size', () => {
        const entry = createFileEntry({ permissions: 0, size: undefined })
        expect(isPermissionDenied(entry)).toBe(true)
    })

    it('returns false for file with no permissions but has size', () => {
        const entry = createFileEntry({ permissions: 0, size: 100 })
        expect(isPermissionDenied(entry)).toBe(false)
    })
})

describe('sizeTierClasses', () => {
    it('has correct classes in order', () => {
        expect(sizeTierClasses).toEqual(['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb'])
    })
})
