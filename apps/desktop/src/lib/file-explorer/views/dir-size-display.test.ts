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
    hasSizeMismatch,
} from './full-list-utils'

// Mock settings store (required by full-list-utils)
vi.mock('$lib/settings/settings-store', () => ({
    getSetting: vi.fn(() => 20),
}))

// Test helpers
const formatSize = (bytes: number): string => `${String(bytes)} bytes`
const formatNum = (n: number): string => String(n)
const plural = (count: number, singular: string, pluralForm: string): string => (count === 1 ? singular : pluralForm)

/** Extracts the html from a tooltip result, or returns the string as-is */
function tooltipHtml(result: string | { html: string }): string {
    return typeof result === 'object' ? result.html : result
}

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

    it('returns "size" when data available and not scanning', () => {
        expect(getDirSizeDisplayState(1234, false)).toBe('size')
    })

    it('returns "size-stale" when data available and scanning', () => {
        expect(getDirSizeDisplayState(1234, true)).toBe('size-stale')
    })

    it('returns "size" for zero size when not scanning', () => {
        expect(getDirSizeDisplayState(0, false)).toBe('size')
    })

    it('returns "size-stale" for zero size when scanning', () => {
        expect(getDirSizeDisplayState(0, true)).toBe('size-stale')
    })

    it('handles undefined recursiveSize correctly regardless of scanning state', () => {
        expect(getDirSizeDisplayState(undefined, false)).toBe('dir')
        expect(getDirSizeDisplayState(undefined, true)).toBe('scanning')
    })
})

// ============================================================================
// buildDirSizeTooltip
// ============================================================================

describe('buildDirSizeTooltip', () => {
    it('returns empty string when no data and not scanning', () => {
        expect(buildDirSizeTooltip(undefined, undefined, 0, 0, false, formatSize, formatNum, plural)).toBe('')
    })

    it('returns "Scanning..." when no data and scanning is active', () => {
        expect(buildDirSizeTooltip(undefined, undefined, 0, 0, true, formatSize, formatNum, plural)).toBe('Scanning...')
    })

    it('returns HTML tooltip with size and counts when recursive size is available', () => {
        const result = buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum, plural)
        const html = tooltipHtml(result)
        expect(html).toContain('1234 bytes')
        expect(html).toContain('10 files')
        expect(html).toContain('3 folders')
        expect(html).toContain('<br>')
    })

    it('appends stale warning when scanning with existing data', () => {
        const result = buildDirSizeTooltip(1234, undefined, 10, 3, true, formatSize, formatNum, plural)
        const html = tooltipHtml(result)
        expect(html).toContain('1234 bytes')
        expect(html).toContain('10 files')
        expect(html).toContain('Updating index')
    })

    it('uses singular form for 1 file', () => {
        const html = tooltipHtml(buildDirSizeTooltip(100, undefined, 1, 5, false, formatSize, formatNum, plural))
        expect(html).toContain('1 file')
        expect(html).not.toContain('1 files')
    })

    it('uses singular form for 1 folder', () => {
        const html = tooltipHtml(buildDirSizeTooltip(100, undefined, 5, 1, false, formatSize, formatNum, plural))
        expect(html).toContain('1 folder')
        expect(html).not.toContain('1 folders')
    })

    it('uses "No files" and "no folders" for zero counts', () => {
        const html = tooltipHtml(buildDirSizeTooltip(100, undefined, 0, 0, false, formatSize, formatNum, plural))
        expect(html).toContain('No files')
        expect(html).toContain('no folders')
    })

    it('handles zero-size directory correctly', () => {
        const html = tooltipHtml(buildDirSizeTooltip(0, undefined, 0, 0, false, formatSize, formatNum, plural))
        expect(html).toContain('0 bytes')
        expect(html).toContain('No files')
        expect(html).toContain('no folders')
    })

    it('handles zero-size directory while scanning', () => {
        const html = tooltipHtml(buildDirSizeTooltip(0, undefined, 0, 0, true, formatSize, formatNum, plural))
        expect(html).toContain('0 bytes')
        expect(html).toContain('Updating index')
    })

    it('handles large file and folder counts', () => {
        const html = tooltipHtml(
            buildDirSizeTooltip(1_000_000_000, undefined, 50000, 1200, false, formatSize, formatNum, plural),
        )
        expect(html).toContain('1000000000 bytes')
        expect(html).toContain('50000 files')
        expect(html).toContain('1200 folders')
    })

    it('uses provided formatSize function', () => {
        const customFormat = (bytes: number): string => `${(bytes / 1024).toFixed(1)} KB`
        const html = tooltipHtml(buildDirSizeTooltip(2048, undefined, 3, 1, false, customFormat, formatNum, plural))
        expect(html).toContain('2.0 KB')
    })

    it('always shows both sizes when physical is available', () => {
        const result = buildDirSizeTooltip(1000000, 1000005, 10, 3, false, formatSize, formatNum, plural)
        const html = tooltipHtml(result)
        expect(html).toContain('Content:')
        expect(html).toContain('1000000 bytes')
        expect(html).toContain('On disk:')
        expect(html).toContain('1000005 bytes')
    })

    it('shows both sizes when physical differs significantly', () => {
        const result = buildDirSizeTooltip(1000000, 800000, 10, 3, false, formatSize, formatNum, plural)
        const html = tooltipHtml(result)
        expect(html).toContain('Content:')
        expect(html).toContain('1000000 bytes')
        expect(html).toContain('On disk:')
        expect(html).toContain('800000 bytes')
        expect(html).toContain('<br>')
    })

    it('shows single size when physical is unavailable', () => {
        const html = tooltipHtml(buildDirSizeTooltip(1000000, undefined, 10, 3, false, formatSize, formatNum, plural))
        expect(html).toContain('1000000 bytes')
        expect(html).not.toContain('Content:')
        expect(html).not.toContain('On disk:')
    })

    it('includes colored triad spans in HTML output', () => {
        const html = tooltipHtml(buildDirSizeTooltip(1234567, undefined, 5, 2, false, formatSize, formatNum, plural))
        expect(html).toContain('class="size-')
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
// hasSizeMismatch
// ============================================================================

describe('hasSizeMismatch', () => {
    it('returns false when logical is undefined', () => {
        expect(hasSizeMismatch(undefined, 500_000_000)).toBe(false)
    })

    it('returns false when physical is undefined', () => {
        expect(hasSizeMismatch(500_000_000, undefined)).toBe(false)
    })

    it('returns false when both are undefined', () => {
        expect(hasSizeMismatch(undefined, undefined)).toBe(false)
    })

    it('returns false when logical is zero', () => {
        expect(hasSizeMismatch(0, 500_000_000)).toBe(false)
    })

    it('returns false when physical is zero', () => {
        expect(hasSizeMismatch(500_000_000, 0)).toBe(false)
    })

    it('returns false when sizes are equal', () => {
        expect(hasSizeMismatch(1_000_000_000, 1_000_000_000)).toBe(false)
    })

    it('returns true when both thresholds are met (50% and 200 MB)', () => {
        // 600 MB logical, 300 MB physical → diff = 300 MB (>200 MB), 300/300 = 100% (>50%)
        expect(hasSizeMismatch(600_000_000, 300_000_000)).toBe(true)
        expect(hasSizeMismatch(300_000_000, 600_000_000)).toBe(true)
    })

    it('returns false when only percentage threshold is met but not absolute', () => {
        // 300 MB logical, 100 MB physical → diff = 200 MB, 200/100 = 200% (>50%)
        // BUT diff = 200 MB which is exactly 200_000_000, need >= so this is true
        // Use smaller values: 100 MB vs 50 MB → diff = 50 MB (<200 MB), 50/50 = 100% (>50%)
        expect(hasSizeMismatch(100_000_000, 50_000_000)).toBe(false)
    })

    it('returns false when only absolute threshold is met but not percentage', () => {
        // 1 GB logical, 800 MB physical → diff = 200 MB (>=200 MB), but 200/800 = 25% (<50%)
        expect(hasSizeMismatch(1_000_000_000, 800_000_000)).toBe(false)
    })

    it('returns true at exact boundary (200 MB diff, 50% relative)', () => {
        // 600 MB logical, 400 MB physical → diff = 200 MB (>=200 MB), 200/400 = 50% (>=50%)
        expect(hasSizeMismatch(600_000_000, 400_000_000)).toBe(true)
    })

    it('returns false just under the absolute boundary', () => {
        // 599_999_999 logical, 399_999_999 physical → diff = 200_000_000, 200M/400M = 50%
        // Actually that's at boundary. Use: diff = 199_999_999
        // 599_999_999 logical, 400_000_000 physical → diff = 199_999_999 (<200 MB)
        expect(hasSizeMismatch(599_999_999, 400_000_000)).toBe(false)
    })
})

// ============================================================================
// buildFileSizeTooltip
// ============================================================================

describe('buildFileSizeTooltip', () => {
    it('returns empty string when both sizes undefined', () => {
        expect(buildFileSizeTooltip(undefined, undefined, formatSize)).toBe('')
    })

    it('returns HTML tooltip when only logical is available', () => {
        const result = buildFileSizeTooltip(1024, undefined, formatSize)
        const html = tooltipHtml(result)
        expect(html).toContain('1024 bytes')
        expect(html).toContain('class="size-')
    })

    it('returns HTML tooltip when only physical is available', () => {
        const result = buildFileSizeTooltip(undefined, 2048, formatSize)
        const html = tooltipHtml(result)
        expect(html).toContain('2048 bytes')
    })

    it('always shows both sizes when both are available', () => {
        const result = buildFileSizeTooltip(1000000, 800000, formatSize)
        const html = tooltipHtml(result)
        expect(html).toContain('Content:')
        expect(html).toContain('1000000 bytes')
        expect(html).toContain('On disk:')
        expect(html).toContain('800000 bytes')
        expect(html).toContain('<br>')
    })

    it('shows both sizes even when sizes are similar', () => {
        const html = tooltipHtml(buildFileSizeTooltip(1000000, 1000005, formatSize))
        expect(html).toContain('Content:')
        expect(html).toContain('1000000 bytes')
        expect(html).toContain('On disk:')
        expect(html).toContain('1000005 bytes')
    })

    it('shows both sizes even when sizes are equal', () => {
        const html = tooltipHtml(buildFileSizeTooltip(500, 500, formatSize))
        expect(html).toContain('Content:')
        expect(html).toContain('On disk:')
    })
})
