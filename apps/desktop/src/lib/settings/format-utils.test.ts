import { describe, it, expect } from 'vitest'
import { formatDateTimeWithFormat, formatFileSizeWithFormat } from './format-utils'

// Fixed timestamp: 2024-03-15 14:30:45 UTC
// We use a local date to avoid timezone flakiness
const fixedDate = new Date(2024, 2, 15, 14, 30, 45) // March 15, 2024 14:30:45 local
const timestamp = fixedDate.getTime() / 1000

describe('formatDateTimeWithFormat', () => {
    it('returns empty string for undefined timestamp', () => {
        expect(formatDateTimeWithFormat(undefined, 'iso', '')).toBe('')
    })

    it('formats as ISO (YYYY-MM-DD HH:mm)', () => {
        expect(formatDateTimeWithFormat(timestamp, 'iso', '')).toBe('2024-03-15 14:30')
    })

    it('formats as short (MM/DD HH:mm)', () => {
        expect(formatDateTimeWithFormat(timestamp, 'short', '')).toBe('03/15 14:30')
    })

    it('formats with custom format string', () => {
        expect(formatDateTimeWithFormat(timestamp, 'custom', 'YYYY/MM/DD HH:mm:ss')).toBe('2024/03/15 14:30:45')
    })

    it('handles custom format with partial tokens', () => {
        expect(formatDateTimeWithFormat(timestamp, 'custom', 'YYYY-MM')).toBe('2024-03')
    })

    it('falls back to ISO for unknown format', () => {
        expect(formatDateTimeWithFormat(timestamp, 'unknown' as never, '')).toBe('2024-03-15 14:30')
    })

    it('formats system locale (returns non-empty string)', () => {
        const result = formatDateTimeWithFormat(timestamp, 'system', '')
        expect(result.length).toBeGreaterThan(0)
    })
})

describe('formatFileSizeWithFormat', () => {
    describe('binary (base 1024)', () => {
        it('formats 0 bytes', () => {
            expect(formatFileSizeWithFormat(0, 'binary')).toBe('0 bytes')
        })

        it('formats bytes below 1 KB', () => {
            expect(formatFileSizeWithFormat(512, 'binary')).toBe('512 bytes')
        })

        it('formats exactly 1 KB', () => {
            expect(formatFileSizeWithFormat(1024, 'binary')).toBe('1.00 KB')
        })

        it('formats megabytes', () => {
            expect(formatFileSizeWithFormat(1024 * 1024, 'binary')).toBe('1.00 MB')
        })

        it('formats gigabytes', () => {
            expect(formatFileSizeWithFormat(1024 ** 3, 'binary')).toBe('1.00 GB')
        })

        it('formats terabytes', () => {
            expect(formatFileSizeWithFormat(1024 ** 4, 'binary')).toBe('1.00 TB')
        })

        it('formats petabytes', () => {
            expect(formatFileSizeWithFormat(1024 ** 5, 'binary')).toBe('1.00 PB')
        })

        it('caps at PB for very large values', () => {
            const result = formatFileSizeWithFormat(1024 ** 6, 'binary')
            expect(result).toBe('1024.00 PB')
        })

        it('formats fractional KB values', () => {
            expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1.50 KB')
        })
    })

    describe('SI (base 1000)', () => {
        it('formats 0 bytes', () => {
            expect(formatFileSizeWithFormat(0, 'si')).toBe('0 bytes')
        })

        it('formats bytes below 1 kB', () => {
            expect(formatFileSizeWithFormat(999, 'si')).toBe('999 bytes')
        })

        it('formats exactly 1 kB', () => {
            expect(formatFileSizeWithFormat(1000, 'si')).toBe('1.00 kB')
        })

        it('formats megabytes', () => {
            expect(formatFileSizeWithFormat(1_000_000, 'si')).toBe('1.00 MB')
        })

        it('formats gigabytes', () => {
            expect(formatFileSizeWithFormat(1_000_000_000, 'si')).toBe('1.00 GB')
        })

        it('uses lowercase k for SI kilo', () => {
            expect(formatFileSizeWithFormat(1500, 'si')).toBe('1.50 kB')
        })
    })

    describe('boundary between binary and SI', () => {
        it('1024 bytes is 1.02 kB in SI', () => {
            expect(formatFileSizeWithFormat(1024, 'si')).toBe('1.02 kB')
        })

        it('1000 bytes is still bytes in binary', () => {
            expect(formatFileSizeWithFormat(1000, 'binary')).toBe('1000 bytes')
        })
    })
})
