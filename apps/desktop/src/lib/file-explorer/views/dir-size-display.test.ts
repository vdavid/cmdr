/**
 * Tests for directory size display logic in FullList.
 * Covers the display states and tooltip building for directories
 * with/without index data and scanning status.
 */
import { describe, it, expect, vi } from 'vitest'
import { getDirSizeDisplayState, buildDirSizeTooltip } from './full-list-utils'

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
        expect(buildDirSizeTooltip(undefined, 0, 0, false, formatSize, formatNum, plural)).toBe('')
    })

    it('returns "Scanning..." when no data and scanning is active', () => {
        expect(buildDirSizeTooltip(undefined, 0, 0, true, formatSize, formatNum, plural)).toBe('Scanning...')
    })

    it('returns formatted size info when recursive size is available', () => {
        const tooltip = buildDirSizeTooltip(1234, 10, 3, false, formatSize, formatNum, plural)
        expect(tooltip).toBe('1234 bytes \u00B7 10 files \u00B7 3 folders')
    })

    it('appends stale warning when scanning with existing data', () => {
        const tooltip = buildDirSizeTooltip(1234, 10, 3, true, formatSize, formatNum, plural)
        expect(tooltip).toContain('1234 bytes')
        expect(tooltip).toContain('10 files')
        expect(tooltip).toContain('3 folders')
        expect(tooltip).toContain('Might be outdated. Currently scanning...')
    })

    it('uses singular form for 1 file', () => {
        const tooltip = buildDirSizeTooltip(100, 1, 5, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('1 file')
        expect(tooltip).not.toContain('1 files')
    })

    it('uses singular form for 1 folder', () => {
        const tooltip = buildDirSizeTooltip(100, 5, 1, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('1 folder')
        expect(tooltip).not.toContain('1 folders')
    })

    it('uses plural form for 0 files', () => {
        const tooltip = buildDirSizeTooltip(100, 0, 0, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('0 files')
        expect(tooltip).toContain('0 folders')
    })

    it('handles zero-size directory correctly', () => {
        const tooltip = buildDirSizeTooltip(0, 0, 0, false, formatSize, formatNum, plural)
        expect(tooltip).toBe('0 bytes \u00B7 0 files \u00B7 0 folders')
    })

    it('handles zero-size directory while scanning', () => {
        const tooltip = buildDirSizeTooltip(0, 0, 0, true, formatSize, formatNum, plural)
        expect(tooltip).toContain('0 bytes')
        expect(tooltip).toContain('Might be outdated')
    })

    it('handles large file and folder counts', () => {
        const tooltip = buildDirSizeTooltip(1_000_000_000, 50000, 1200, false, formatSize, formatNum, plural)
        expect(tooltip).toContain('1000000000 bytes')
        expect(tooltip).toContain('50000 files')
        expect(tooltip).toContain('1200 folders')
    })

    it('uses provided formatSize function', () => {
        const customFormat = (bytes: number): string => `${(bytes / 1024).toFixed(1)} KB`
        const tooltip = buildDirSizeTooltip(2048, 3, 1, false, customFormat, formatNum, plural)
        expect(tooltip).toContain('2.0 KB')
    })
})
