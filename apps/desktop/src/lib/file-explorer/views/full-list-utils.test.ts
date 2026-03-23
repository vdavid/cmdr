/**
 * Tests for full-list-utils.ts
 */
import { describe, it, expect, vi } from 'vitest'
import { getVisibleItemsCount, FULL_LIST_ROW_HEIGHT, getVirtualizationBufferRows } from './full-list-utils'

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
