/**
 * Tests for directory size display logic in FullList.
 * Covers the display states and tooltip building for directories
 * with/without index data and scanning status.
 */
import { describe, it, expect, vi } from 'vitest'
import {
    getDirSizeDisplayState,
    buildDirSizeTooltip,
    getDisplaySize,
    buildFileSizeTooltip,
    sizesDifferSignificantly,
} from './full-list-utils'

// Mock settings store (required by full-list-utils)
vi.mock('$lib/settings/settings-store', () => ({
    getSetting: vi.fn().mockReturnValue(20),
}))

// ============================================================================
// getDirSizeDisplayState
// ============================================================================

describe('getDirSizeDisplayState', () => {
    it('returns "dir" when no data and not scanning', () => {
        expect(getDirSizeDisplayState(undefined, false)).toBe('dir')
    })

    it('returns "scanning" when no data and scanning is active', () => {
        expect(getDirSizeDisplayState(undefined, true)).toBe('scanning')
    })

    it('returns "size" when recursive size is available and not scanning', () => {
        expect(getDirSizeDisplayState(1234567, false)).toBe('size')
    })

    it('returns "size-stale" when recursive size is available and scanning is active', () => {
        expect(getDirSizeDisplayState(1234567, true)).toBe('size-stale')
    })

    it('returns "size" for zero-size directory when not scanning', () => {
        expect(getDirSizeDisplayState(0, false)).toBe('size')
    })

    it('returns "size-stale" for zero-size directory when scanning', () => {
        expect(getDirSizeDisplayState(0, true)).toBe('size-stale')
    })

    it('returns "size" for very large sizes', () => {
        expect(getDirSizeDisplayState(1_000_000_000_000, false)).toBe('size')
    })
})

// ============================================================================
// buildDirSizeTooltip
// ============================================================================

// Simple formatters for testing (mirrors the real ones but deterministic)
const formatSize = (bytes: number): string => `${String(bytes)} bytes`
const formatNum = (n: number): string => String(n)
const plural = (count: number, singular: string, pluralForm: string): string => (count === 1 ? singular : pluralForm)

describe('buildDirSizeTooltip', () => {
    it('returns empty string when no data and not scanning', () => {
        expect(buildDirSizeTooltip(undefined, undefined, 0, 0, false, formatSize, formatNum, plural)).toBe('')
    })

    it('returns "Scanning..." when no data and scanning is active', () => {
        expect(buildDirSizeTooltip(undefined, undefined, 0, 0, true, formatSize, formatNum, plural)).toBe('Scanning...')
    })

    it('returns formatted size info when recursive size is available', () => {
        const tooltip = buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum, plural)
        expect(tooltip).toBe('1234 bytes \u00B7 10 files \u00B7 3 folders')
    })

    it('appends stale warning when scanning with existing data', () => {
        const tooltip = buildDirSizeTooltip(1234, undefined, 10, 3, true, formatSize, formatNum, plural)
        expect(tooltip).toContain('1234 bytes')
        expect(tooltip).toContain('10 files')
        expect(tooltip).toContain('3 folders')
        expect(tooltip).toContain('Might be outdated. Currently scanning...')
    })

    it('uses singular form for 1 file', () => {
        const tooltip = buildDirSizeTooltip(100, undefined, 1, 5, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('1 file')
        expect(tooltip).not.toContain('1 files')
    })

    it('uses singular form for 1 folder', () => {
        const tooltip = buildDirSizeTooltip(100, undefined, 5, 1, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('1 folder')
        expect(tooltip).not.toContain('1 folders')
    })

    it('uses plural form for 0 files', () => {
        const tooltip = buildDirSizeTooltip(100, undefined, 0, 0, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('0 files')
        expect(tooltip).toContain('0 folders')
    })

    it('handles zero-size directory correctly', () => {
        const tooltip = buildDirSizeTooltip(0, undefined, 0, 0, false, formatSize, formatNum, plural)
        expect(tooltip).toBe('0 bytes \u00B7 0 files \u00B7 0 folders')
    })

    it('handles zero-size directory while scanning', () => {
        const tooltip = buildDirSizeTooltip(0, undefined, 0, 0, true, formatSize, formatNum, plural)
        expect(tooltip).toContain('0 bytes')
        expect(tooltip).toContain('Might be outdated')
    })

    it('handles large file and folder counts', () => {
        const tooltip = buildDirSizeTooltip(1_000_000_000, undefined, 50000, 1200, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('1000000000 bytes')
        expect(tooltip).toContain('50000 files')
        expect(tooltip).toContain('1200 folders')
    })

    it('uses provided formatSize function', () => {
        const customFormat = (bytes: number): string => `${(bytes / 1024).toFixed(1)} KB`
        const tooltip = buildDirSizeTooltip(2048, undefined, 3, 1, false, customFormat, formatNum, plural)
        expect(tooltip).toContain('2.0 KB')
    })

    it('shows both sizes when physical differs significantly', () => {
        const tooltip = buildDirSizeTooltip(1000000, 800000, 10, 3, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('Content: 1000000 bytes')
        expect(tooltip).toContain('On disk: 800000 bytes')
    })

    it('shows single size when physical is similar', () => {
        const tooltip = buildDirSizeTooltip(1000000, 1000005, 10, 3, false, formatSize, formatNum, plural)
        expect(tooltip).toBe('1000000 bytes \u00B7 10 files \u00B7 3 folders')
    })
})

// ============================================================================
// getDisplaySize
// ============================================================================

describe('getDisplaySize', () => {
    it('returns logical in logical mode', () => {
        expect(getDisplaySize(100, 200, 'logical')).toBe(100)
    })

    it('returns physical in physical mode', () => {
        expect(getDisplaySize(100, 200, 'physical')).toBe(200)
    })

    it('returns min in smart mode when both available', () => {
        expect(getDisplaySize(100, 200, 'smart')).toBe(100)
        expect(getDisplaySize(200, 100, 'smart')).toBe(100)
    })

    it('returns logical when physical is undefined in smart mode', () => {
        expect(getDisplaySize(100, undefined, 'smart')).toBe(100)
    })

    it('returns physical when logical is undefined in smart mode', () => {
        expect(getDisplaySize(undefined, 200, 'smart')).toBe(200)
    })

    it('falls back to logical when physical is undefined in physical mode', () => {
        expect(getDisplaySize(100, undefined, 'physical')).toBe(100)
    })

    it('returns undefined when both are undefined', () => {
        expect(getDisplaySize(undefined, undefined, 'smart')).toBeUndefined()
        expect(getDisplaySize(undefined, undefined, 'logical')).toBeUndefined()
        expect(getDisplaySize(undefined, undefined, 'physical')).toBeUndefined()
    })
})

// ============================================================================
// sizesDifferSignificantly
// ============================================================================

describe('sizesDifferSignificantly', () => {
    it('returns false when both are zero', () => {
        expect(sizesDifferSignificantly(0, 0)).toBe(false)
    })

    it('returns true when one is zero and the other is not', () => {
        expect(sizesDifferSignificantly(0, 100)).toBe(true)
        expect(sizesDifferSignificantly(100, 0)).toBe(true)
    })

    it('returns false when values are equal', () => {
        expect(sizesDifferSignificantly(1000, 1000)).toBe(false)
    })

    it('returns false when difference is at the 1% boundary', () => {
        // 1% of 10000 = 100; difference of 100 → ratio = 0.01 → not >0.01
        expect(sizesDifferSignificantly(10000, 10100)).toBe(false)
    })

    it('returns true when difference exceeds 1%', () => {
        // difference = 200, larger = 10200, ratio = 200/10200 ≈ 0.0196 > 0.01
        expect(sizesDifferSignificantly(10000, 10200)).toBe(true)
        expect(sizesDifferSignificantly(10200, 10000)).toBe(true)
    })

    it('returns true for large relative difference', () => {
        expect(sizesDifferSignificantly(100, 200)).toBe(true)
    })
})

// ============================================================================
// buildFileSizeTooltip
// ============================================================================

describe('buildFileSizeTooltip', () => {
    it('returns empty string when both sizes undefined', () => {
        expect(buildFileSizeTooltip(undefined, undefined, formatSize)).toBe('')
    })

    it('returns single size when only logical is available', () => {
        expect(buildFileSizeTooltip(1024, undefined, formatSize)).toBe('1024 bytes')
    })

    it('returns single size when only physical is available', () => {
        expect(buildFileSizeTooltip(undefined, 2048, formatSize)).toBe('2048 bytes')
    })

    it('shows both when sizes differ significantly', () => {
        const tooltip = buildFileSizeTooltip(1000000, 800000, formatSize)
        expect(tooltip).toBe('Content: 1000000 bytes \u00B7 On disk: 800000 bytes')
    })

    it('shows single size when sizes are similar', () => {
        const tooltip = buildFileSizeTooltip(1000000, 1000005, formatSize)
        expect(tooltip).toBe('1000000 bytes')
    })

    it('shows single size when sizes are equal', () => {
        expect(buildFileSizeTooltip(500, 500, formatSize)).toBe('500 bytes')
    })
})
