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

    it('shows both sizes on separate lines when physical differs significantly', () => {
        const result = buildDirSizeTooltip(1000000, 800000, 10, 3, false, formatSize, formatNum, plural)
        const html = tooltipHtml(result)
        expect(html).toContain('Content:')
        expect(html).toContain('1000000 bytes')
        expect(html).toContain('On disk:')
        expect(html).toContain('800000 bytes')
        // Both size lines should be on separate lines from counts
        expect(html).toContain('<br>')
    })

    it('shows single size when physical is similar', () => {
        const html = tooltipHtml(buildDirSizeTooltip(1000000, 1000005, 10, 3, false, formatSize, formatNum, plural))
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

    it('shows both sizes on separate lines when they differ significantly', () => {
        const result = buildFileSizeTooltip(1000000, 800000, formatSize)
        const html = tooltipHtml(result)
        expect(html).toContain('Content:')
        expect(html).toContain('1000000 bytes')
        expect(html).toContain('On disk:')
        expect(html).toContain('800000 bytes')
        expect(html).toContain('<br>')
    })

    it('shows single size when sizes are similar', () => {
        const html = tooltipHtml(buildFileSizeTooltip(1000000, 1000005, formatSize))
        expect(html).toContain('1000000 bytes')
        expect(html).not.toContain('Content:')
    })

    it('shows single size when sizes are equal', () => {
        const html = tooltipHtml(buildFileSizeTooltip(500, 500, formatSize))
        expect(html).toContain('500 bytes')
        expect(html).not.toContain('Content:')
    })
})
