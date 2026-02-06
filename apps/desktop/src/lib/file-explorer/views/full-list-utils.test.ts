/**
 * Tests for full-list-utils.ts
 */
import { describe, it, expect, vi } from 'vitest'
import {
    getVisibleItemsCount,
    formatDateShort,
    FULL_LIST_ROW_HEIGHT,
    getVirtualizationBufferRows,
} from './full-list-utils'

// Mock the settings store
vi.mock('$lib/settings/settings-store', () => ({
    getSetting: vi.fn().mockReturnValue(20), // Default buffer size
}))

describe('constants', () => {
    it('has expected row height', () => {
        expect(FULL_LIST_ROW_HEIGHT).toBe(20)
    })

    it('has expected buffer size from settings', () => {
        expect(getVirtualizationBufferRows()).toBe(20)
    })
})

describe('getVisibleItemsCount', () => {
    it('calculates visible items with default row height', () => {
        expect(getVisibleItemsCount(400)).toBe(20) // 400 / 20 = 20
    })

    it('rounds up partial items', () => {
        expect(getVisibleItemsCount(410)).toBe(21) // ceil(410 / 20) = 21
    })

    it('handles exact multiple', () => {
        expect(getVisibleItemsCount(200)).toBe(10)
    })

    it('handles small container', () => {
        expect(getVisibleItemsCount(15)).toBe(1) // ceil(15 / 20) = 1
    })

    it('handles zero height', () => {
        expect(getVisibleItemsCount(0)).toBe(0)
    })

    it('accepts custom row height', () => {
        expect(getVisibleItemsCount(400, 40)).toBe(10) // 400 / 40 = 10
    })

    it('calculates with custom row height and rounding', () => {
        expect(getVisibleItemsCount(410, 40)).toBe(11) // ceil(410 / 40) = 11
    })
})

describe('formatDateShort', () => {
    it('returns empty string for undefined', () => {
        expect(formatDateShort(undefined)).toBe('')
    })

    it('formats Unix timestamp correctly', () => {
        // The result depends on local timezone, but format should be YYYY-MM-DD HH:MM
        const timestamp = 1705322445
        const result = formatDateShort(timestamp)
        expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/)
    })

    it('formats epoch timestamp', () => {
        const result = formatDateShort(0)
        expect(result).toMatch(/^1970-01-01/)
    })

    it('pads single digit values', () => {
        // Jan 1, 2021 00:05:00 UTC = timestamp 1609459500
        // Format should include leading zeros
        const result = formatDateShort(1609459500)
        expect(result).toMatch(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}/)
    })

    it('does NOT include seconds (unlike SelectionInfo format)', () => {
        const timestamp = 1705322445
        const result = formatDateShort(timestamp)
        // Should be HH:MM format, not HH:MM:SS
        const parts = result.split(' ')
        expect(parts[1]).toMatch(/^\d{2}:\d{2}$/)
    })
})
